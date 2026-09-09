use crate::{FontReference, normalize_name};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// A collection face is identified by both its canonical path and index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FontFace {
    pub path: PathBuf,
    pub face_index: u32,
    pub names: Vec<String>,
    /// Family aliases (name IDs 1/16/21), used for weight/italic filtering.
    pub family_names: Vec<String>,
    /// Typed internal names for reporting match provenance. Untyped `names`
    /// supplied by callers are reported as `internal_name`.
    pub name_records: Vec<FontName>,
    pub weight: u16,
    /// Includes fonts marked as oblique.
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    Family,
    FullName,
    PostScriptName,
    InternalName,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FontName {
    pub name: String,
    pub kind: NameKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanIssue {
    pub path: PathBuf,
    pub face_index: Option<u32>,
    pub message: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanReport {
    pub faces: Vec<FontFace>,
    pub issues: Vec<ScanIssue>,
}

impl ScanReport {
    /// Recursively scan explicit files/directories; do not follow directory symlinks.
    /// Unreadable and invalid files are reported without aborting the scan.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    NameNotFound,
    StyleNotFound,
}

/// All name records that matched a candidate under the selected matching rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchEvidence {
    pub path: PathBuf,
    pub face_index: u32,
    pub matched_names: Vec<FontName>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub reference: FontReference,
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

#[derive(Debug, Default, Clone, Serialize)]
pub struct ResolveReport {
    pub resolved: Vec<Resolution>,
    pub missing: Vec<Resolution>,
    pub ambiguous: Vec<Resolution>,
}

/// Reusable index of all supported internal names, with deterministic ordering.
#[derive(Debug, Default)]
pub struct FontIndex {
    faces: Vec<FontFace>,
    names: BTreeMap<String, BTreeSet<usize>>,
    families: BTreeMap<String, BTreeSet<usize>>,
}

impl FontIndex {
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
                    face.name_records.push(FontName {
                        name: name.clone(),
                        kind: NameKind::Family,
                    });
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
        for (id, face) in faces.iter().enumerate() {
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
        }
    }

    /// Family aliases require exact weight and italic matches. Other aliases
    /// identify concrete faces regardless of requested styling. If an alias is
    /// both a family and a full name, the family interpretation takes precedence.
    /// No nearest-weight fallback, synthesis, or variable-font instancing occurs.
    pub fn resolve(&self, references: &[FontReference]) -> ResolveReport {
        let mut report = ResolveReport::default();
        for reference in references {
            let key = normalize_name(&reference.name);
            let family = self.families.get(&key);
            let candidates = family
                .or_else(|| self.names.get(&key))
                .into_iter()
                .flatten()
                .filter(|&&i| {
                    family.is_none()
                        || (self.faces[i].weight == reference.weight
                            && self.faces[i].italic == reference.italic)
                })
                .map(|&i| self.faces[i].clone())
                .collect::<Vec<_>>();
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
                    matched_names: face
                        .name_records
                        .iter()
                        .filter(|r| {
                            normalize_name(&r.name) == key
                                && (family.is_none() || r.kind == NameKind::Family)
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
