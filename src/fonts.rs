use crate::{FontReference, normalize_name};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Owned metadata for one face in a font file or collection.
///
/// Identity is `(path, face_index)`, not the name or file contents. No font bytes
/// are retained. Scanned names include Unicode records and Macintosh Roman ASCII;
/// unsupported legacy encodings are skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FontFace {
    /// Canonical file path when produced by [`ScanReport::scan`].
    pub path: PathBuf,
    /// Zero-based collection index; standalone TTF/OTF files use zero.
    pub face_index: u32,
    /// All decoded matching aliases, sorted and deduplicated by the scanner.
    pub names: Vec<String>,
    /// Family aliases (name IDs 1/16/21), used for weight/italic filtering.
    pub family_names: Vec<String>,
    /// Typed internal names for reporting match provenance. Untyped `names`
    /// supplied by callers are reported as `internal_name`.
    pub name_records: Vec<FontName>,
    /// Native numeric weight from the parser (normally 1–1000); 400 if OS/2 is absent.
    /// This value is not changed by matching or synthesis.
    pub weight: u16,
    /// Includes fonts marked as oblique.
    pub italic: bool,
}

/// Semantic role of an internal alias; related OpenType IDs share a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    /// Legacy, typographic, or WWS family (IDs 1, 16, 21).
    Family,
    /// Full or compatible full name (IDs 4, 18).
    FullName,
    /// PostScript name (ID 6).
    PostScriptName,
    /// Caller-supplied alias without known name-table provenance.
    InternalName,
}

/// An original decoded alias and its classification.
///
/// For caller-built records, keep `kind` consistent with `name_id`, or use
/// `name_id: None` when the original ID is unknown. The index does not validate
/// contradictory caller-supplied metadata.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FontName {
    /// Original decoded spelling, before matching normalization.
    pub name: String,
    /// Name role used for selection and evidence reporting.
    pub kind: NameKind,
    /// Original OpenType name ID; absent for caller-supplied aliases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_id: Option<u16>,
}

/// A filesystem or face parsing problem encountered while scanning.
#[derive(Debug, Clone, Serialize)]
pub struct ScanIssue {
    /// Affected file or directory; canonicalization may not have succeeded.
    pub path: PathBuf,
    /// Affected face, or `None` for a filesystem or collection-level failure.
    pub face_index: Option<u32>,
    /// Human-readable error, not a stable machine-readable code.
    pub message: String,
}

/// Successfully parsed faces and non-fatal scan issues.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanReport {
    /// Faces sorted by canonical path and collection index.
    pub faces: Vec<FontFace>,
    /// Problems encountered; inspect even when matching reports no missing fonts.
    pub issues: Vec<ScanIssue>,
}

impl ScanReport {
    /// Recursively scan explicit files/directories; do not follow directory symlinks.
    /// Unreadable and invalid files are reported without aborting the scan.
    ///
    /// Accepts TTF/OTF/TTC/OTC extensions case-insensitively, and scans every
    /// collection face. Canonical paths deduplicate overlapping roots and file
    /// symlinks. Other extensions are ignored. No system directories are added.
    ///
    /// Each file is read fully into memory, then released after metadata extraction.
    /// A parsed face without usable names is retained and produces an issue.
    /// Nonexistent roots and traversal errors also populate [`Self::issues`].
    ///
    /// ```no_run
    /// let scan = ass_fonts::ScanReport::scan(["./fonts", "./extra/font.ttc"]);
    /// for face in &scan.faces {
    ///     println!("{}#{}: {:?}", face.path.display(), face.face_index, face.names);
    /// }
    /// assert!(scan.issues.is_empty(), "{:?}", scan.issues);
    /// ```
    pub fn scan<I, P>(roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut report = Self::default();
        let mut paths = BTreeSet::new();
        for root in roots {
            for entry in walkdir::WalkDir::new(root.as_ref()).follow_links(false) {
                match entry {
                    Ok(entry) if entry.file_type().is_file() || entry.path().is_file() => {
                        let path = entry.path();
                        let supported =
                            path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                                ["ttf", "otf", "ttc", "otc"]
                                    .contains(&x.to_ascii_lowercase().as_str())
                            });
                        if supported {
                            match fs::canonicalize(path) {
                                Ok(path) => {
                                    paths.insert(path);
                                }
                                Err(e) => report.issue(path, None, e.to_string()),
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => report.issue(e.path().unwrap_or(root.as_ref()), None, e.to_string()),
                }
            }
        }
        for path in paths {
            match fs::read(&path) {
                Ok(data) => report.scan_data(&path, &data),
                Err(e) => report.issue(&path, None, e.to_string()),
            }
        }
        report
    }

    fn issue(&mut self, path: &Path, face_index: Option<u32>, message: String) {
        self.issues.push(ScanIssue {
            path: path.to_owned(),
            face_index,
            message,
        });
    }

    fn scan_data(&mut self, path: &Path, data: &[u8]) {
        let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
        // A collection must have room for every four-byte face offset.
        if count == 0 || (count > 1 && count as usize > data.len().saturating_sub(12) / 4) {
            self.issue(path, None, "invalid collection face count".into());
            return;
        }
        for face_index in 0..count {
            let face = match ttf_parser::Face::parse(data, face_index) {
                Ok(face) => face,
                Err(e) => {
                    self.issue(path, Some(face_index), e.to_string());
                    continue;
                }
            };
            let mut names = BTreeSet::new();
            let mut family_names = BTreeSet::new();
            let mut name_records = BTreeSet::new();
            for name in face.names() {
                // Family, full, PostScript, typographic family, compatible full, WWS family.
                if ![1, 4, 6, 16, 18, 21].contains(&name.name_id) {
                    continue;
                }
                let decoded = name.to_string().or_else(|| {
                    // Macintosh Roman ASCII is safe; do not misdecode legacy high bytes.
                    (name.platform_id == ttf_parser::PlatformId::Macintosh
                        && name.encoding_id == 0
                        && name.name.is_ascii())
                    .then(|| String::from_utf8_lossy(name.name).into_owned())
                });
                if let Some(decoded) = decoded.filter(|n| !normalize_name(n).is_empty()) {
                    if [1, 16, 21].contains(&name.name_id) {
                        family_names.insert(decoded.clone());
                    }
                    name_records.insert(FontName {
                        name: decoded.clone(),
                        name_id: Some(name.name_id),
                        kind: match name.name_id {
                            1 | 16 | 21 => NameKind::Family,
                            4 | 18 => NameKind::FullName,
                            6 => NameKind::PostScriptName,
                            _ => unreachable!(),
                        },
                    });
                    names.insert(decoded);
                }
            }
            if names.is_empty() {
                self.issue(path, Some(face_index), "no decodable matching names".into());
            }
            self.faces.push(FontFace {
                path: path.to_owned(),
                face_index,
                names: names.into_iter().collect(),
                family_names: family_names.into_iter().collect(),
                name_records: name_records.into_iter().collect(),
                weight: face.weight().to_number(),
                italic: face.is_italic() || face.is_oblique(),
            });
        }
    }
}

/// Why no candidate could be selected under the requested options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    /// No normalized internal alias matched the requested name.
    NameNotFound,
    /// The family exists, but no face satisfies the current weight/italic policy.
    StyleNotFound,
}

/// All name records that matched a candidate under the selected matching rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchEvidence {
    /// Same path as the corresponding candidate.
    pub path: PathBuf,
    /// Same collection index as the corresponding candidate.
    pub face_index: u32,
    /// Matching name records that support the selected method, in sorted order.
    pub matched_names: Vec<FontName>,
    /// True only when bold synthesis was permitted and the 400→700 rule applies.
    /// False does not guarantee visual fidelity or that a renderer will not synthesize.
    /// No font is generated and actual rendering is not checked.
    pub synthetic_bold: bool,
    /// True when italic synthesis is permitted and an italic request selects an
    /// upright face. No glyphs are transformed by this library.
    pub synthetic_italic: bool,
    /// How this candidate was selected, independently of the synthesis flag.
    pub selection_method: SelectionMethod,
}

/// Selection path used for a candidate; serialized using snake_case names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMethod {
    /// Exact normalized PostScript alias, ahead of family and full-name matches.
    PostScriptName,
    /// Full name sufficiently distinct from generic family aliases.
    FullName,
    /// Legacy family alias belonging to a broader typographic family and one style.
    LegacyFamilyName,
    /// Family candidate with exact requested weight and italic state.
    FamilyExact,
    /// Family candidate at minimum weight distance in the selected slant tier.
    FamilyNearest,
    /// A permitted bold or italic fallback selected this face without weight approximation.
    FamilySynthesis,
    /// An alias without a more specific selection classification.
    InternalName,
}

/// Weight selection policy for generic families, not specific face names.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WeightMatching {
    #[default]
    /// Require numeric equality. Separate synthesis permission can still allow fallback.
    Exact,
    /// Minimum absolute weight difference among faces in the selected family/slant tier.
    /// All equally close faces are retained; there is no distance cutoff.
    /// Exact matches are preferred and reported as [`SelectionMethod::FamilyExact`].
    /// This absolute-distance policy does not emulate CSS or a specific renderer.
    Nearest,
}

/// Permitted renderer-side synthesis, disabled by default.
///
/// Permission records a dependency on synthesis; this library does not alter glyphs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SynthesisPolicy {
    /// Permit weight 400 to satisfy weight 700. Can combine with italic synthesis.
    /// No other synthetic weight conversions are implemented.
    pub bold: bool,
    /// Permit an upright face for an italic request if no native-italic candidate
    /// satisfies the weight policy. Never substitutes italic for upright text.
    pub italic: bool,
}

/// Independent controls for family weight selection and style synthesis.
///
/// Defaults are [`WeightMatching::Exact`] and no synthesis. The options do not
/// change alias normalization or specific-name selection.
///
/// ```
/// use ass_fonts::{ResolveOptions, SynthesisPolicy, WeightMatching};
/// let options = ResolveOptions {
///     weight_matching: WeightMatching::Nearest,
///     synthesis: SynthesisPolicy { bold: true, italic: true },
/// };
/// assert_eq!(options.weight_matching, WeightMatching::Nearest);
/// assert!(!ResolveOptions::default().synthesis.bold);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResolveOptions {
    /// Exact or minimum-distance selection within a matching family.
    pub weight_matching: WeightMatching,
    /// Synthesis permission evaluated independently of weight selection.
    pub synthesis: SynthesisPolicy,
}

impl From<ResolveMode> for ResolveOptions {
    fn from(mode: ResolveMode) -> Self {
        Self {
            synthesis: SynthesisPolicy {
                bold: mode == ResolveMode::AllowStyleSynthesis,
                italic: mode == ResolveMode::AllowStyleSynthesis,
            },
            ..Self::default()
        }
    }
}

/// Compatibility presets for [`FontIndex::resolve_with_mode`].
///
/// Both presets use exact weight matching. Use [`ResolveOptions`] to enable nearest
/// weight selection; converting a preset with `ResolveOptions::from` preserves this.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveMode {
    #[default]
    /// Exact family matching with synthesis disabled.
    Strict,
    /// Allow both renderer-side bold (400→700) and italic (upright→italic) synthesis.
    /// Exact family matches always take precedence. This is a collection policy,
    /// not an emulation of a particular renderer's font selection algorithm.
    AllowStyleSynthesis,
}

/// One input reference and its selection result.
///
/// Produced entries in [`ResolveReport`] have zero, one, or multiple candidates
/// according to their bucket. Optional missing information is omitted from JSON
/// when absent; empty `candidates` and `matches` arrays are retained.
#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    /// Original request, including its source lines and requested attributes.
    pub reference: FontReference,
    /// Selected native faces, sorted by path and face index.
    pub candidates: Vec<FontFace>,
    /// Present only for missing entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_reason: Option<MissingReason>,
    /// All scanned faces of the matching family, only for `style_not_found`.
    /// These are informational, not successful candidates or fallbacks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available_variants: Vec<FontFace>,
    /// One entry per candidate, in the same order. Empty for missing entries.
    pub matches: Vec<MatchEvidence>,
}

/// Requests partitioned by candidate count, preserving input order in each bucket.
///
/// Matching does not deduplicate input references. A resolved entry identifies a
/// dependency, not glyph coverage or exact rendering. Inspect scan and subtitle
/// diagnostics separately; they are not embedded in this report.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ResolveReport {
    /// Exactly one candidate per entry.
    pub resolved: Vec<Resolution>,
    /// No candidates; each entry contains a [`MissingReason`].
    pub missing: Vec<Resolution>,
    /// Two or more candidates, including equally close weights and duplicate files.
    pub ambiguous: Vec<Resolution>,
}

impl ResolveReport {
    /// Check nominal glyph coverage for every selected candidate.
    ///
    /// This reopens font files and is equivalent to [`crate::check_coverage`].
    /// See that function for cmap and rendering limitations.
    pub fn check_coverage(&self) -> crate::CoverageReport {
        crate::check_coverage(self)
    }
}

/// Reusable index of all supported internal names, with deterministic ordering.
#[derive(Debug, Default)]
pub struct FontIndex {
    faces: Vec<FontFace>,
    names: BTreeMap<String, BTreeSet<usize>>,
    families: BTreeMap<String, BTreeSet<usize>>,
    full_names: BTreeMap<String, BTreeSet<usize>>,
    postscript_names: BTreeMap<String, BTreeSet<usize>>,
    generic_families: BTreeSet<String>,
}

impl FontIndex {
    /// Build an in-memory index from owned face metadata; performs no filesystem I/O.
    ///
    /// Merges aliases for identical `(path, face_index)` pairs. If duplicate inputs
    /// disagree on weight or italic state, the first input's attributes are retained.
    /// Caller paths are not canonicalized; use [`ScanReport::scan`] for canonical paths.
    /// Typed records populate the alias lists, and untyped aliases receive
    /// [`NameKind::InternalName`] evidence. Candidates are ordered by path and index.
    ///
    /// The index can be reused for many subtitles and different resolution options.
    pub fn new(faces: impl IntoIterator<Item = FontFace>) -> Self {
        let mut merged: BTreeMap<(PathBuf, u32), FontFace> = BTreeMap::new();
        for face in faces {
            let existing = merged
                .entry((face.path.clone(), face.face_index))
                .or_insert_with(|| face.clone());
            existing.names.extend(face.names);
            existing.family_names.extend(face.family_names);
            existing.name_records.extend(face.name_records);
        }
        let faces: Vec<_> = merged
            .into_values()
            .map(|mut face| {
                for record in &face.name_records {
                    face.names.push(record.name.clone());
                    if record.kind == NameKind::Family {
                        face.family_names.push(record.name.clone());
                    }
                }
                for name in &face.family_names {
                    if !face
                        .name_records
                        .iter()
                        .any(|r| r.name == *name && r.kind == NameKind::Family)
                    {
                        face.name_records.push(FontName {
                            name: name.clone(),
                            kind: NameKind::Family,
                            name_id: None,
                        });
                    }
                }
                face.names.extend(face.family_names.iter().cloned());
                face.names.sort();
                face.names.dedup();
                face.family_names.sort();
                face.family_names.dedup();
                for name in &face.names {
                    if !face.name_records.iter().any(|r| r.name == *name) {
                        face.name_records.push(FontName {
                            name: name.clone(),
                            kind: NameKind::InternalName,
                            name_id: None,
                        });
                    }
                }
                face.name_records.sort();
                face.name_records.dedup();
                face
            })
            .collect();
        let mut names: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut families: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut full_names: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut postscript_names: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut generic_families = BTreeSet::new();
        for (id, face) in faces.iter().enumerate() {
            let has_typographic = face.name_records.iter().any(|r| r.name_id == Some(16));
            for record in &face.name_records {
                let key = normalize_name(&record.name);
                if key.is_empty() {
                    continue;
                }
                match record.kind {
                    NameKind::FullName => {
                        full_names.entry(key).or_default().insert(id);
                    }
                    NameKind::PostScriptName => {
                        postscript_names.entry(key).or_default().insert(id);
                    }
                    NameKind::Family
                        if record.name_id == Some(16)
                            || record.name_id == Some(21)
                            || !has_typographic =>
                    {
                        generic_families.insert(key);
                    }
                    _ => {}
                }
            }
            for name in &face.family_names {
                let key = normalize_name(name);
                if !key.is_empty() {
                    families.entry(key).or_default().insert(id);
                }
            }
            for name in &face.names {
                let key = normalize_name(name);
                if !key.is_empty() {
                    names.entry(key).or_default().insert(id);
                }
            }
        }
        Self {
            faces,
            names,
            families,
            full_names,
            postscript_names,
            generic_families,
        }
    }

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
            let key = normalize_name(&reference.name);
            let family = self.families.get(&key);
            let full = self.full_names.get(&key).filter(|ids| {
                family.is_none()
                    || (!self.generic_families.contains(&key)
                        && family.is_some_and(|family| family.is_subset(ids)))
            });
            let (selected, mut method) = if let Some(ids) = self.postscript_names.get(&key) {
                (Some(ids), SelectionMethod::PostScriptName)
            } else if let Some(ids) = full {
                (Some(ids), SelectionMethod::FullName)
            } else if let Some(ids) = family.filter(|ids| {
                !self.generic_families.contains(&key)
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
                (self.names.get(&key), SelectionMethod::InternalName)
            };
            let family_match = method == SelectionMethod::FamilyExact;
            let mut candidates = selected
                .into_iter()
                .flatten()
                .filter(|&&i| {
                    !family_match
                        || (self.faces[i].weight == reference.weight
                            && self.faces[i].italic == reference.italic)
                })
                .map(|&i| self.faces[i].clone())
                .collect::<Vec<_>>();
            if candidates.is_empty()
                && family_match
                && options.weight_matching == WeightMatching::Nearest
            {
                let eligible = || {
                    family
                        .into_iter()
                        .flatten()
                        .map(|&i| &self.faces[i])
                        .filter(|face| face.italic == reference.italic)
                };
                if let Some(distance) = eligible()
                    .map(|face| face.weight.abs_diff(reference.weight))
                    .min()
                {
                    candidates = eligible()
                        .filter(|face| face.weight.abs_diff(reference.weight) == distance)
                        .cloned()
                        .collect();
                    method = SelectionMethod::FamilyNearest;
                }
            }
            if candidates.is_empty() && family_match && options.synthesis.bold {
                candidates = family
                    .into_iter()
                    .flatten()
                    .map(|&i| &self.faces[i])
                    .filter(|face| {
                        face.italic == reference.italic && needs_synthetic_bold(reference, face)
                    })
                    .cloned()
                    .collect();
                if !candidates.is_empty() {
                    method = SelectionMethod::FamilySynthesis;
                }
            }
            if candidates.is_empty() && family_match && options.synthesis.italic && reference.italic
            {
                let upright: Vec<_> = family
                    .into_iter()
                    .flatten()
                    .map(|&i| &self.faces[i])
                    .filter(|face| !face.italic)
                    .collect();
                candidates = upright
                    .iter()
                    .filter(|face| face.weight == reference.weight)
                    .map(|face| (*face).clone())
                    .collect();
                if !candidates.is_empty() {
                    method = SelectionMethod::FamilySynthesis;
                } else if options.weight_matching == WeightMatching::Nearest {
                    if let Some(distance) = upright
                        .iter()
                        .map(|face| face.weight.abs_diff(reference.weight))
                        .min()
                    {
                        candidates = upright
                            .iter()
                            .filter(|face| face.weight.abs_diff(reference.weight) == distance)
                            .map(|face| (*face).clone())
                            .collect();
                        method = SelectionMethod::FamilyNearest;
                    }
                } else if options.synthesis.bold {
                    candidates = upright
                        .into_iter()
                        .filter(|face| needs_synthetic_bold(reference, face))
                        .cloned()
                        .collect();
                    if !candidates.is_empty() {
                        method = SelectionMethod::FamilySynthesis;
                    }
                }
            }
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
                .map(|face| MatchEvidence {
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
                                    SelectionMethod::PostScriptName => {
                                        r.kind == NameKind::PostScriptName
                                    }
                                    SelectionMethod::FullName => r.kind == NameKind::FullName,
                                    SelectionMethod::LegacyFamilyName => {
                                        r.kind == NameKind::Family && r.name_id == Some(1)
                                    }
                                    SelectionMethod::FamilyExact
                                    | SelectionMethod::FamilyNearest
                                    | SelectionMethod::FamilySynthesis => {
                                        r.kind == NameKind::Family
                                    }
                                    SelectionMethod::InternalName => true,
                                }
                        })
                        .cloned()
                        .collect(),
                })
                .collect();
            let resolution = Resolution {
                reference: reference.clone(),
                candidates,
                missing_reason,
                available_variants,
                matches,
            };
            match count {
                0 => report.missing.push(resolution),
                1 => report.resolved.push(resolution),
                _ => report.ambiguous.push(resolution),
            }
        }
        report
    }
}

fn needs_synthetic_bold(reference: &FontReference, face: &FontFace) -> bool {
    reference.weight == 700 && face.weight == 400
}
