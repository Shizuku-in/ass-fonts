use super::*;
use std::{fs, path::Path};

/// Version of the scanning output for an unchanged font file.
///
/// Changes whenever [`scan_font_file`] can produce different faces or issues
/// for the same bytes, including when the `legacy-names` feature is toggled.
/// Store it with cached [`FontFace`] values and discard caches that differ.
pub const SCAN_FORMAT_VERSION: u32 = if cfg!(feature = "legacy-names") { 2 } else { 1 };

/// Progress of [`ScanReport::scan_with`], reported after each font file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScanProgress<'a> {
    /// Files finished so far, including this one.
    pub completed: usize,
    /// Font files found by discovery.
    pub total: usize,
    /// The file that was just scanned.
    pub path: &'a Path,
}

/// Find font files below explicit files/directories without parsing them.
///
/// Recurses into directories without following directory symlinks. Accepts
/// TTF/OTF/TTC/OTC extensions case-insensitively; other files are ignored. Paths
/// are canonicalized, which also deduplicates overlapping roots and file
/// symlinks, and returned sorted. Nonexistent roots, traversal errors, and
/// canonicalization failures are returned as issues.
///
/// Together with [`scan_font_file`], this lets callers reuse cached faces for
/// unchanged files; [`ScanReport::scan`] is equivalent to scanning every path.
pub fn discover_font_files<I, P>(roots: I) -> (Vec<PathBuf>, Vec<ScanIssue>)
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut paths = BTreeSet::new();
    let mut issues = Vec::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root.as_ref()).follow_links(false) {
            match entry {
                Ok(entry) if entry.file_type().is_file() || entry.path().is_file() => {
                    let path = entry.path();
                    let supported = path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                        ["ttf", "otf", "ttc", "otc"].contains(&x.to_ascii_lowercase().as_str())
                    });
                    if supported {
                        match fs::canonicalize(path) {
                            Ok(path) => {
                                paths.insert(path);
                            }
                            Err(e) => issues.push(issue(path, None, e.to_string())),
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => issues.push(issue(
                    e.path().unwrap_or(root.as_ref()),
                    None,
                    e.to_string(),
                )),
            }
        }
    }
    (paths.into_iter().collect(), issues)
}

/// Read one font file and extract metadata for every face it contains.
///
/// `path` is stored in the faces as given; [`discover_font_files`] returns
/// canonical paths. The file is read fully into memory and released afterwards.
/// A read failure, an invalid collection, or a face that cannot be parsed is
/// reported as an issue. A parsed face without usable names is retained and
/// also produces an issue, as does a legacy name record that cannot be decoded.
pub fn scan_font_file(path: &Path) -> (Vec<FontFace>, Vec<ScanIssue>) {
    match fs::read(path) {
        Ok(data) => scan_data(path, &data),
        Err(e) => (Vec::new(), vec![issue(path, None, e.to_string())]),
    }
}

impl ScanReport {
    /// Recursively scan explicit files/directories; do not follow directory symlinks.
    /// Unreadable and invalid files are reported without aborting the scan.
    ///
    /// Equivalent to [`discover_font_files`] followed by [`scan_font_file`] for
    /// each path, with discovery issues first. Every collection face is scanned.
    /// No system directories are added.
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
        Self::scan_with(roots, |_| {})
    }

    /// Like [`Self::scan`], reporting progress after each font file.
    ///
    /// The callback always runs on the calling thread. With the `parallel`
    /// feature, files are scanned concurrently and progress arrives in
    /// completion order, but the report is identical to a serial scan.
    ///
    /// ```no_run
    /// let scan = ass_fonts::ScanReport::scan_with(["./fonts"], |progress| {
    ///     eprintln!("{}/{} {}", progress.completed, progress.total, progress.path.display());
    /// });
    /// ```
    pub fn scan_with<I, P>(roots: I, mut progress: impl FnMut(ScanProgress<'_>)) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let (paths, issues) = discover_font_files(roots);
        let mut report = Self {
            faces: Vec::new(),
            issues,
        };
        for (faces, issues) in scan_files(&paths, &mut progress) {
            report.faces.extend(faces);
            report.issues.extend(issues);
        }
        report
    }
}

type FileScan = (Vec<FontFace>, Vec<ScanIssue>);

#[cfg(not(feature = "parallel"))]
fn scan_files(paths: &[PathBuf], progress: &mut impl FnMut(ScanProgress<'_>)) -> Vec<FileScan> {
    let total = paths.len();
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let result = scan_font_file(path);
            progress(ScanProgress {
                completed: index + 1,
                total,
                path,
            });
            result
        })
        .collect()
}

#[cfg(feature = "parallel")]
fn scan_files(paths: &[PathBuf], progress: &mut impl FnMut(ScanProgress<'_>)) -> Vec<FileScan> {
    use rayon::prelude::*;
    let total = paths.len();
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = scope.spawn(move || {
            paths
                .par_iter()
                .enumerate()
                .map(|(index, path)| {
                    let result = scan_font_file(path);
                    // The receiver outlives the worker, so sending cannot fail.
                    let _ = sender.send(index);
                    result
                })
                .collect::<Vec<_>>()
        });
        for (completed, index) in receiver.iter().enumerate() {
            progress(ScanProgress {
                completed: completed + 1,
                total,
                path: &paths[index],
            });
        }
        worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn issue(path: &Path, face_index: Option<u32>, message: String) -> ScanIssue {
    ScanIssue {
        path: path.to_owned(),
        face_index,
        message,
    }
}

fn scan_data(path: &Path, data: &[u8]) -> FileScan {
    let mut faces = Vec::new();
    let mut issues = Vec::new();
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    // A collection must have room for every four-byte face offset.
    if count == 0 || (count > 1 && count as usize > data.len().saturating_sub(12) / 4) {
        issues.push(issue(path, None, "invalid collection face count".into()));
        return (faces, issues);
    }
    for face_index in 0..count {
        let face = match ttf_parser::Face::parse(data, face_index) {
            Ok(face) => face,
            Err(e) => {
                issues.push(issue(path, Some(face_index), e.to_string()));
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
            let decoded = match decode_name(&name) {
                Ok(decoded) => decoded,
                Err(message) => {
                    issues.push(issue(path, Some(face_index), message));
                    continue;
                }
            };
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
            issues.push(issue(
                path,
                Some(face_index),
                "no decodable matching names".into(),
            ));
        }
        faces.push(FontFace {
            path: path.to_owned(),
            face_index,
            names: names.into_iter().collect(),
            family_names: family_names.into_iter().collect(),
            name_records: name_records.into_iter().collect(),
            weight: face.weight().to_number(),
            italic: face.is_italic() || face.is_oblique(),
        });
    }
    (faces, issues)
}

/// Decode a name record; `Ok(None)` skips an unsupported encoding silently.
fn decode_name(name: &ttf_parser::name::Name<'_>) -> Result<Option<String>, String> {
    if let Some(decoded) = name.to_string() {
        return Ok(Some(decoded));
    }
    #[cfg(feature = "legacy-names")]
    if let Some(decoded) = legacy::decode(name) {
        return decoded.map(Some);
    }
    // Macintosh Roman ASCII is safe; do not misdecode legacy high bytes.
    Ok((name.platform_id == ttf_parser::PlatformId::Macintosh
        && name.encoding_id == 0
        && name.name.is_ascii())
    .then(|| String::from_utf8_lossy(name.name).into_owned()))
}

#[cfg(feature = "legacy-names")]
mod legacy {
    use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, MACINTOSH, SHIFT_JIS};
    use ttf_parser::{PlatformId, name::Name};

    /// Decode a CJK legacy record, or `None` if its encoding is not handled here.
    pub(super) fn decode(name: &Name<'_>) -> Option<Result<String, String>> {
        let (encoding, wide): (&'static Encoding, bool) = match (name.platform_id, name.encoding_id)
        {
            (PlatformId::Windows, 2) => (SHIFT_JIS, true),
            (PlatformId::Windows, 3) => (GBK, true),
            (PlatformId::Windows, 4) => (BIG5, true),
            (PlatformId::Windows, 5) => (EUC_KR, true),
            (PlatformId::Windows, 6) => {
                return Some(Err(format!(
                    "unsupported Johab name record (name ID {})",
                    name.name_id
                )));
            }
            (PlatformId::Macintosh, 0) => (MACINTOSH, false),
            (PlatformId::Macintosh, 1) => (SHIFT_JIS, false),
            (PlatformId::Macintosh, 2) => (BIG5, false),
            (PlatformId::Macintosh, 3) => (EUC_KR, false),
            (PlatformId::Macintosh, 25) => (GBK, false),
            _ => return None,
        };
        let bytes = if wide {
            unpack(name.name)
        } else {
            name.name.to_vec()
        };
        Some(
            encoding
                .decode_without_bom_handling_and_without_replacement(&bytes)
                .map(|text| text.trim_end_matches('\0').to_owned())
                .ok_or_else(|| {
                    format!(
                        "cannot decode name ID {} (platform {:?}, encoding {}) as {}",
                        name.name_id,
                        name.platform_id,
                        name.encoding_id,
                        encoding.name()
                    )
                }),
        )
    }

    /// Windows legacy records store each character as a big-endian 16-bit unit,
    /// so single-byte characters carry a zero high byte. Drop those zero bytes;
    /// odd-length records are already plain multibyte strings.
    fn unpack(bytes: &[u8]) -> Vec<u8> {
        if !bytes.len().is_multiple_of(2) {
            return bytes.to_vec();
        }
        bytes
            .chunks_exact(2)
            .flat_map(|unit| if unit[0] == 0 { &unit[1..] } else { unit })
            .copied()
            .collect()
    }
}
