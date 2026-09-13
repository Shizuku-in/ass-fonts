use super::*;

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
}
