use super::*;
use std::{fs, path::Path};

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
