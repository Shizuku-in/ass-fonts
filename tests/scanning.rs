use ass_fonts::{
    FontFace, FontIndex, FontReference, SCAN_FORMAT_VERSION, ScanReport, discover_font_files,
    scan_font_file,
};

#[path = "support/font.rs"]
mod font_fixture;
use font_fixture::{Spec, font, sfnt_with_records};

fn spec<'a>(names: &'a [(u16, &'a str)]) -> Spec<'a> {
    Spec {
        names,
        characters: &['a'],
        weight: 400,
        italic: false,
        otf: false,
        base: 0,
    }
}

#[test]
fn discovery_and_single_file_scans_compose_into_scan() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.ttf"), font("Beta", &['b'])).unwrap();
    std::fs::write(dir.path().join("a.OTF"), font("Alpha", &['a'])).unwrap();
    std::fs::write(dir.path().join("broken.ttc"), b"not a font").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"ignored").unwrap();
    let missing = dir.path().join("missing");
    let roots = [dir.path().to_owned(), dir.path().to_owned(), missing];

    let (paths, discovery_issues) = discover_font_files(&roots);
    let names: Vec<_> = paths.iter().map(|p| p.file_name().unwrap()).collect();
    assert_eq!(names, ["a.OTF", "b.ttf", "broken.ttc"]);
    assert_eq!(discovery_issues.len(), 1);

    let mut composed = ScanReport {
        faces: vec![],
        issues: discovery_issues,
    };
    for path in &paths {
        let (faces, issues) = scan_font_file(path);
        composed.faces.extend(faces);
        composed.issues.extend(issues);
    }
    let scanned = ScanReport::scan(&roots);
    assert_eq!(scanned, composed);
    assert_eq!(scanned.faces.len(), 2);
    assert_eq!(scanned.issues.len(), 2);

    let mut progress = vec![];
    let reported = ScanReport::scan_with(&roots, |p| {
        progress.push((p.completed, p.total, p.path.to_owned()))
    });
    assert_eq!(reported, scanned);
    let completed: Vec<_> = progress.iter().map(|p| (p.0, p.1)).collect();
    assert_eq!(completed, [(1, 3), (2, 3), (3, 3)]);
    let mut seen: Vec<_> = progress.into_iter().map(|(_, _, path)| path).collect();
    seen.sort();
    assert_eq!(seen, paths);
}

#[test]
fn scan_reports_round_trip_through_serde() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.ttf"), font("Alpha", &['a'])).unwrap();
    std::fs::write(dir.path().join("b.ttf"), b"broken").unwrap();
    let report = ScanReport::scan([dir.path()]);
    let cached = serde_json::json!({
        "version": SCAN_FORMAT_VERSION,
        "report": report,
    });
    let text = serde_json::to_string(&cached).unwrap();
    let restored: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(restored["version"], SCAN_FORMAT_VERSION);
    let restored: ScanReport = serde_json::from_value(restored["report"].clone()).unwrap();
    assert_eq!(restored, report);
    let faces: Vec<FontFace> =
        serde_json::from_str(&serde_json::to_string(&report.faces).unwrap()).unwrap();
    assert_eq!(faces, report.faces);
}

#[test]
fn index_exposes_faces_lookup_and_injected_aliases() {
    let face = |path: &str, family: &str, weight| FontFace {
        path: path.into(),
        face_index: 0,
        names: vec![],
        family_names: vec![family.into()],
        name_records: vec![],
        weight,
        italic: false,
    };
    let regular = face("song.ttf", "宋体", 400);
    // Inject a fallback alias into an already scanned face.
    let alias = FontFace {
        names: vec!["宋体_GBK".into()],
        family_names: vec![],
        weight: 900,
        ..regular.clone()
    };
    let index = FontIndex::new([face("bold.ttf", "宋体", 700), regular, alias]);
    assert_eq!(index.faces().len(), 2);
    assert_eq!(index.faces()[1].names, ["宋体", "宋体_GBK"]);
    assert_eq!(
        index.faces()[1].weight,
        400,
        "first attributes are retained"
    );
    assert_eq!(index.lookup(" 宋体 ").len(), 2);
    assert_eq!(
        index.lookup("@宋体_gbk")[0].path,
        std::path::Path::new("song.ttf")
    );
    assert!(index.lookup("absent").is_empty());

    let reference = FontReference {
        name: "宋体_GBK".into(),
        weight: 400,
        ..Default::default()
    };
    let report = index.resolve(&[reference]);
    assert_eq!(report.resolved.len(), 1);
    assert_eq!(
        report.resolved[0].candidates[0].path,
        std::path::Path::new("song.ttf")
    );
}

#[test]
fn legacy_name_records_are_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.ttf");
    let bytes = sfnt_with_records(
        spec(&[(6, "Legacy-Regular")]),
        &[
            // Windows PRC "宋体A", stored as 16-bit units with a zero high byte for ASCII.
            (3, 3, 4, &[0xcb, 0xce, 0xcc, 0xe5, 0x00, 0x41]),
            // Windows Big5 "細明體".
            (3, 4, 1, &[0xb2, 0xd3, 0xa9, 0xfa, 0xc5, 0xe9]),
            // Macintosh Simplified Chinese "黑体".
            (1, 25, 16, &[0xba, 0xda, 0xcc, 0xe5]),
            // Invalid GBK: a lead byte without a trail byte.
            (1, 25, 1, &[0x81]),
        ],
    );
    std::fs::write(&path, bytes).unwrap();
    let scan = ScanReport::scan([&path]);
    let face = &scan.faces[0];
    if cfg!(feature = "legacy-names") {
        assert_eq!(face.names, ["Legacy-Regular", "宋体A", "細明體", "黑体"]);
        assert_eq!(face.family_names, ["細明體", "黑体"]);
        assert_eq!(scan.issues.len(), 1, "{:?}", scan.issues);
        assert_eq!(scan.issues[0].face_index, Some(0));
        assert!(scan.issues[0].message.contains("GBK"));
        let index = FontIndex::new(scan.faces.clone());
        assert_eq!(index.lookup("宋体a").len(), 1);
    } else {
        assert_eq!(face.names, ["Legacy-Regular"]);
        assert!(scan.issues.is_empty());
    }
}
