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
                if let Some(name) = decoded.filter(|n| !normalize_name(n).is_empty()) {
                    names.insert(name);
                }
            }
            if names.is_empty() {
                self.issue(path, Some(face_index), "no decodable matching names".into());
            }
            self.faces.push(FontFace {
                path: path.to_owned(),
                face_index,
                names: names.into_iter().collect(),
            });
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub reference: FontReference,
    pub candidates: Vec<FontFace>,
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
}

impl FontIndex {
    pub fn new(faces: impl IntoIterator<Item = FontFace>) -> Self {
        let mut merged: BTreeMap<(PathBuf, u32), BTreeSet<String>> = BTreeMap::new();
        for face in faces {
            merged
                .entry((face.path, face.face_index))
                .or_default()
                .extend(face.names);
        }
        let faces: Vec<_> = merged
            .into_iter()
            .map(|((path, face_index), names)| FontFace {
                path,
                face_index,
                names: names.into_iter().collect(),
            })
            .collect();
        let mut names: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (id, face) in faces.iter().enumerate() {
            for name in &face.names {
                let key = normalize_name(name);
                if !key.is_empty() {
                    names.entry(key).or_default().insert(id);
                }
            }
        }
        Self { faces, names }
    }

    pub fn resolve(&self, references: &[FontReference]) -> ResolveReport {
        let mut report = ResolveReport::default();
        for reference in references {
            let candidates = self
                .names
                .get(&normalize_name(&reference.name))
                .into_iter()
                .flatten()
                .map(|&i| self.faces[i].clone())
                .collect::<Vec<_>>();
            let count = candidates.len();
            let resolution = Resolution {
                reference: reference.clone(),
                candidates,
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
