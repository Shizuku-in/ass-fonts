use super::*;

impl FontIndex {
    /// Resolve with exact family weight matching and no style synthesis.
    /// Specific names select faces independently of their native weight.
    ///
    /// Equivalent to [`Self::resolve_with_options`] with [`ResolveOptions::default`].
    /// References are processed in input order; no additional files are scanned.
    pub fn resolve(&self, references: &[FontReference]) -> ResolveReport {
        self.resolve_with_mode(references, ResolveMode::Strict)
    }

    /// Resolve with an explicit policy. Synthetic bold uses only weight 400 for
    /// weight 700, never a Light face. Italic synthesis allows upright faces. Tied normal
    /// faces remain ambiguous. Specific-name matches keep their existing face
    /// selection, and are marked if this same synthesis rule applies.
    pub fn resolve_with_mode(
        &self,
        references: &[FontReference],
        mode: ResolveMode,
    ) -> ResolveReport {
        self.resolve_with_options(references, mode.into())
    }

    /// PostScript names have priority. Full names override legacy family aliases
    /// only when name-table relationships distinguish them from generic families.
    /// With insufficient metadata, family interpretation is retained.
    /// Within each slant tier, nearest weight selection precedes bold fallback. It does not
    /// itself enable synthesis or change the matching of specific names.
    ///
    /// # Selection order
    ///
    /// 1. A matching PostScript name locates a specific face.
    /// 2. A full name locates a face unless it also denotes a generic family.
    /// 3. A legacy family alias can locate a variant when a broader typographic
    ///    family exists and all matching faces agree on weight and italic state.
    /// 4. Generic families use exact attributes, then optional nearest weight,
    ///    then optional synthetic-bold fallback in the native slant tier. If no
    ///    candidate remains and italic synthesis is permitted, repeat weight
    ///    selection among upright faces for an italic request. Never reverse this.
    /// 5. Remaining untyped aliases locate their associated faces directly.
    ///
    /// Missing or conflicting name-table metadata keeps the conservative family
    /// interpretation. Specific-name matches retain native attributes even when
    /// these differ from the request; they do not certify the requested visual style.
    ///
    /// # Reporting
    ///
    /// All equally ranked candidates are kept, never resolved by path order.
    /// Nearest selection and synthesis are reported independently: selecting 693
    /// for 700 is `family_nearest` without synthetic bold; selecting 400 for 700
    /// sets the synthesis flag only when permission is enabled. Names that cannot
    /// be found and families lacking an eligible style have distinct missing reasons.
    ///
    /// This method performs no I/O, font modification, or renderer invocation.
    pub fn resolve_with_options(
        &self,
        references: &[FontReference],
        options: ResolveOptions,
    ) -> ResolveReport {
        let mut report = ResolveReport::default();
        for reference in references {
            let resolution = self.resolve_reference(reference, options);
            match resolution.candidates.len() {
                0 => report.missing.push(resolution),
                1 => report.resolved.push(resolution),
                _ => report.ambiguous.push(resolution),
            }
        }
        report
    }

    fn select_alias(&self, key: &str) -> (Option<&BTreeSet<usize>>, SelectionMethod) {
        let family = self.families.get(key);
        let full = self.full_names.get(key).filter(|ids| {
            family.is_none()
                || (!self.generic_families.contains(key)
                    && family.is_some_and(|family| family.is_subset(ids)))
        });
        if let Some(ids) = self.postscript_names.get(key) {
            (Some(ids), SelectionMethod::PostScriptName)
        } else if let Some(ids) = full {
            (Some(ids), SelectionMethod::FullName)
        } else if let Some(ids) = family.filter(|ids| {
            !self.generic_families.contains(key)
                && ids
                    .iter()
                    .map(|&i| (self.faces[i].weight, self.faces[i].italic))
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 1
        }) {
            // A legacy subfamily of a broader typographic family can have
            // a full-name spelling different from its family alias. Only
            // treat it as specific when its faces agree on style.
            (Some(ids), SelectionMethod::LegacyFamilyName)
        } else if let Some(ids) = family {
            (Some(ids), SelectionMethod::FamilyExact)
        } else {
            (self.names.get(key), SelectionMethod::InternalName)
        }
    }

    fn resolve_reference(&self, reference: &FontReference, options: ResolveOptions) -> Resolution {
        let key = normalize_name(&reference.name);
        let family = self.families.get(&key);
        let (selected, mut method) = self.select_alias(&key);
        let candidates = if method == SelectionMethod::FamilyExact {
            let faces: Vec<_> = selected
                .into_iter()
                .flatten()
                .map(|&i| &self.faces[i])
                .collect();
            let (candidates, selected_method) = select_family(&faces, reference, options);
            method = selected_method;
            candidates.into_iter().cloned().collect::<Vec<_>>()
        } else {
            selected
                .into_iter()
                .flatten()
                .map(|&i| self.faces[i].clone())
                .collect()
        };
        let count = candidates.len();
        let missing_reason = (count == 0).then_some(if family.is_some() {
            MissingReason::StyleNotFound
        } else {
            MissingReason::NameNotFound
        });
        let available_variants = if missing_reason == Some(MissingReason::StyleNotFound) {
            family
                .into_iter()
                .flatten()
                .map(|&i| self.faces[i].clone())
                .collect()
        } else {
            Vec::new()
        };
        let matches = candidates
            .iter()
            .map(|face| match_evidence(face, reference, &key, method, options))
            .collect();
        Resolution {
            reference: reference.clone(),
            candidates,
            missing_reason,
            available_variants,
            matches,
        }
    }
}

// Try the native slant first; upright fallback requires explicit italic synthesis.
fn select_family<'a>(
    faces: &[&'a FontFace],
    reference: &FontReference,
    options: ResolveOptions,
) -> (Vec<&'a FontFace>, SelectionMethod) {
    let native = select_weight(faces, reference, options, reference.italic);
    if !native.0.is_empty() || !options.synthesis.italic || !reference.italic {
        return native;
    }
    let (candidates, method) = select_weight(faces, reference, options, false);
    let method = if method == SelectionMethod::FamilyExact && !candidates.is_empty() {
        SelectionMethod::FamilySynthesis
    } else {
        method
    };
    (candidates, method)
}

// Apply the same weight precedence within either slant tier, retaining every tie.
fn select_weight<'a>(
    faces: &[&'a FontFace],
    reference: &FontReference,
    options: ResolveOptions,
    italic: bool,
) -> (Vec<&'a FontFace>, SelectionMethod) {
    let eligible = || faces.iter().copied().filter(|face| face.italic == italic);
    let exact: Vec<_> = eligible()
        .filter(|face| face.weight == reference.weight)
        .collect();
    if !exact.is_empty() {
        return (exact, SelectionMethod::FamilyExact);
    }
    if options.weight_matching == WeightMatching::Nearest {
        if let Some(distance) = eligible()
            .map(|face| face.weight.abs_diff(reference.weight))
            .min()
        {
            let nearest = eligible()
                .filter(|face| face.weight.abs_diff(reference.weight) == distance)
                .collect();
            return (nearest, SelectionMethod::FamilyNearest);
        }
    }
    if options.synthesis.bold {
        let synthetic: Vec<_> = eligible()
            .filter(|face| needs_synthetic_bold(reference, face))
            .collect();
        if !synthetic.is_empty() {
            return (synthetic, SelectionMethod::FamilySynthesis);
        }
    }
    (Vec::new(), SelectionMethod::FamilyExact)
}

fn needs_synthetic_bold(reference: &FontReference, face: &FontFace) -> bool {
    reference.weight == 700 && face.weight == 400
}

fn match_evidence(
    face: &FontFace,
    reference: &FontReference,
    key: &str,
    method: SelectionMethod,
    options: ResolveOptions,
) -> MatchEvidence {
    MatchEvidence {
        path: face.path.clone(),
        face_index: face.face_index,
        selection_method: method,
        synthetic_bold: options.synthesis.bold
            && needs_synthetic_bold(reference, face)
            && (face.italic == reference.italic
                || (options.synthesis.italic && reference.italic && !face.italic)),
        synthetic_italic: options.synthesis.italic && reference.italic && !face.italic,
        matched_names: face
            .name_records
            .iter()
            .filter(|r| {
                normalize_name(&r.name) == key
                    && match method {
                        SelectionMethod::PostScriptName => r.kind == NameKind::PostScriptName,
                        SelectionMethod::FullName => r.kind == NameKind::FullName,
                        SelectionMethod::LegacyFamilyName => {
                            r.kind == NameKind::Family && r.name_id == Some(1)
                        }
                        SelectionMethod::FamilyExact
                        | SelectionMethod::FamilyNearest
                        | SelectionMethod::FamilySynthesis => r.kind == NameKind::Family,
                        SelectionMethod::InternalName => true,
                    }
            })
            .cloned()
            .collect(),
    }
}
