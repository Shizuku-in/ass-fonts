use ass_fonts::{
    CoverageUnavailableReason, FontFace, FontIndex, FontReference, MissingReason, ScanReport,
    check_coverage, extract_fonts,
};

#[path = "support/font.rs"]
mod font_fixture;
use font_fixture::font;

fn subtitle(font: &str, first: &str, second: &str) -> String {
    format!(
        "[V4+ Styles]\nFormat: Name,Fontname\nStyle: Default,{font}\n[Events]\nFormat: Style,Text\nDialogue: Default,{first}\nDialogue: Default,{second}\n"
    )
}

#[test]
fn extraction_records_distinct_characters_and_source_lines() {
    let parsed = extract_fonts(&subtitle("Base", r"A中A\N {\fnAlt}B‍️", "中C"));
    let base = parsed
        .references
        .iter()
        .find(|reference| reference.name == "Base")
        .unwrap();
    assert_eq!(base.lines, [6, 7]);
    assert_eq!(base.characters[0].codepoint(), "U+0041");
    assert_eq!(
        ass_fonts::CharacterUsage {
            character: '😀',
            lines: vec![],
        }
        .codepoint(),
        "U+1F600"
    );
    assert_eq!(
        base.characters
            .iter()
            .map(|usage| (usage.character, usage.lines.clone()))
            .collect::<Vec<_>>(),
        [('A', vec![6]), ('C', vec![7]), ('中', vec![6, 7])]
    );
    let alt = parsed
        .references
        .iter()
        .find(|reference| reference.name == "Alt")
        .unwrap();
    assert_eq!(
        alt.characters
            .iter()
            .map(|usage| usage.character)
            .collect::<Vec<_>>(),
        ['B', '\u{200d}', '\u{fe0f}']
    );
}

#[test]
fn ambiguous_candidates_are_checked_independently() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("complete.ttf"),
        font("Example", &['A', '中']),
    )
    .unwrap();
    std::fs::write(dir.path().join("incomplete.ttf"), font("Example", &['A'])).unwrap();
    std::fs::write(
        dir.path().join("also-incomplete.ttf"),
        font("Example", &['A']),
    )
    .unwrap();
    let scan = ScanReport::scan([dir.path()]);
    assert!(scan.issues.is_empty(), "{:?}", scan.issues);
    let parsed = extract_fonts(&subtitle("Example", "A中A", "中"));
    let resolved = FontIndex::new(scan.faces).resolve(&parsed.references);
    assert_eq!(resolved.ambiguous.len(), 1);

    let coverage = resolved.check_coverage();
    assert_eq!(coverage.complete.len(), 1);
    assert_eq!(coverage.incomplete.len(), 2);
    assert!(coverage.uncheckable.is_empty());
    assert_eq!(coverage.complete[0].checked_characters, 2);
    assert_eq!(coverage.incomplete[0].checked_characters, 2);
    assert_eq!(coverage.incomplete[0].missing_characters[0].character, '中');
    assert_eq!(coverage.incomplete[0].missing_characters[0].lines, [6, 7]);
    let summary = coverage.summary();
    assert_eq!(summary.complete_candidates, 1);
    assert_eq!(summary.incomplete_candidates, 2);
    assert_eq!(summary.uncheckable_candidates, 0);
    assert_eq!(summary.unresolved_references, 0);
    assert_eq!(summary.missing_mappings, 2);
    assert_eq!(summary.unique_missing_characters, 1);
    assert_eq!(coverage.missing_characters()[0].character, '中');
    assert_eq!(coverage.missing_characters()[0].lines, [6, 7]);
    assert!(!coverage.is_complete());
}

#[test]
fn format_controls_do_not_require_standalone_glyphs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("example.ttf"), font("Example", &['A'])).unwrap();
    let scan = ScanReport::scan([dir.path()]);
    let parsed = extract_fonts(&subtitle("Example", "A\u{200d}\u{fe0f}", ""));
    let resolved = FontIndex::new(scan.faces).resolve(&parsed.references);
    let coverage = resolved.check_coverage();
    assert_eq!(coverage.complete.len(), 1);
    assert_eq!(coverage.complete[0].checked_characters, 1);
    assert!(coverage.incomplete.is_empty());
}

#[test]
fn missing_and_unreadable_fonts_are_uncheckable() {
    let parsed = extract_fonts(&subtitle("Absent", "A", ""));
    let missing = FontIndex::default().resolve(&parsed.references);
    let coverage = check_coverage(&missing);
    assert_eq!(coverage.uncheckable.len(), 1);
    assert_eq!(
        coverage.uncheckable[0].reason,
        CoverageUnavailableReason::NoCandidate
    );
    assert_eq!(
        coverage.uncheckable[0].missing_reason,
        Some(MissingReason::NameNotFound)
    );
    assert!(coverage.uncheckable[0].candidate.is_none());
    assert_eq!(coverage.summary().unresolved_references, 1);
    assert_eq!(coverage.summary().uncheckable_candidates, 0);
    assert!(!coverage.is_complete());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.ttf");
    std::fs::write(&path, font("Gone", &['A'])).unwrap();
    let scan = ScanReport::scan([&path]);
    let parsed = extract_fonts(&subtitle("Gone", "A", ""));
    let resolved = FontIndex::new(scan.faces).resolve(&parsed.references);
    std::fs::remove_file(path).unwrap();
    let coverage = resolved.check_coverage();
    assert_eq!(coverage.uncheckable.len(), 1);
    assert_eq!(
        coverage.uncheckable[0].reason,
        CoverageUnavailableReason::FontReadError
    );
    assert_eq!(coverage.summary().uncheckable_candidates, 1);
    assert_eq!(coverage.summary().unresolved_references, 0);
}

#[test]
fn invalid_face_index_is_reported_separately_from_read_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.ttf");
    std::fs::write(&path, font("Example", &['A'])).unwrap();
    let face = FontFace {
        path,
        face_index: 1,
        names: vec!["Example".into()],
        family_names: vec![],
        name_records: vec![],
        weight: 400,
        italic: false,
    };
    let reference = FontReference {
        name: "Example".into(),
        weight: 400,
        italic: false,
        lines: vec![1],
        characters: vec![],
    };
    let resolved = FontIndex::new([face]).resolve(&[reference]);
    let coverage = resolved.check_coverage();
    assert_eq!(coverage.uncheckable.len(), 1);
    assert_eq!(
        coverage.uncheckable[0].reason,
        CoverageUnavailableReason::FaceParseError
    );
}
