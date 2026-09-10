use ass_fonts::{
    CoverageUnavailableReason, FontFace, FontIndex, FontReference, MissingReason, ScanReport,
    check_coverage, extract_fonts,
};

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

// A minimal synthetic TrueType font with a format 12 cmap. It is sufficient for
// metadata and cmap tests without relying on installed or redistributable fonts.
fn font(name: &str, characters: &[char]) -> Vec<u8> {
    let mut characters = characters.to_vec();
    characters.sort_unstable();
    characters.dedup();

    let utf16: Vec<u8> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let mut naming = vec![0; 18];
    put16(&mut naming, 2, 1);
    put16(&mut naming, 4, 18);
    put16(&mut naming, 6, 3);
    put16(&mut naming, 8, 1);
    put16(&mut naming, 10, 0x409);
    put16(&mut naming, 12, 1);
    put16(&mut naming, 14, utf16.len() as u16);
    naming.extend(utf16);

    let mut cmap = vec![0; 28 + 12 * characters.len()];
    put16(&mut cmap, 2, 1);
    put16(&mut cmap, 4, 3);
    put16(&mut cmap, 6, 10);
    put32(&mut cmap, 8, 12);
    put16(&mut cmap, 12, 12);
    put32(&mut cmap, 16, (16 + 12 * characters.len()) as u32);
    put32(&mut cmap, 24, characters.len() as u32);
    for (index, character) in characters.iter().enumerate() {
        let offset = 28 + index * 12;
        put32(&mut cmap, offset, *character as u32);
        put32(&mut cmap, offset + 4, *character as u32);
        put32(&mut cmap, offset + 8, (index + 1) as u32);
    }

    let mut head = vec![0; 54];
    put16(&mut head, 18, 1000);
    let mut hhea = vec![0; 36];
    put32(&mut hhea, 0, 0x10000);
    let mut maxp = vec![0; 6];
    put32(&mut maxp, 0, 0x5000);
    put16(&mut maxp, 4, (characters.len() + 1) as u16);
    let mut os2 = vec![0; 78];
    put16(&mut os2, 4, 400);
    put16(&mut os2, 6, 5);

    let tables = [
        (*b"OS/2", os2),
        (*b"cmap", cmap),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"name", naming),
    ];
    let mut bytes = vec![0; 12 + 16 * tables.len()];
    put32(&mut bytes, 0, 0x10000);
    put16(&mut bytes, 4, tables.len() as u16);
    for (index, (tag, table)) in tables.into_iter().enumerate() {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let offset = bytes.len();
        let record = 12 + index * 16;
        bytes[record..record + 4].copy_from_slice(&tag);
        put32(&mut bytes, record + 8, offset as u32);
        put32(&mut bytes, record + 12, table.len() as u32);
        bytes.extend(table);
    }
    bytes
}

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
    let scan = ScanReport::scan([dir.path()]);
    assert!(scan.issues.is_empty(), "{:?}", scan.issues);
    let parsed = extract_fonts(&subtitle("Example", "A中A", "中"));
    let resolved = FontIndex::new(scan.faces).resolve(&parsed.references);
    assert_eq!(resolved.ambiguous.len(), 1);

    let coverage = resolved.check_coverage();
    assert_eq!(coverage.complete.len(), 1);
    assert_eq!(coverage.incomplete.len(), 1);
    assert!(coverage.uncheckable.is_empty());
    assert_eq!(coverage.complete[0].checked_characters, 2);
    assert_eq!(coverage.incomplete[0].checked_characters, 2);
    assert_eq!(coverage.incomplete[0].missing_characters[0].character, '中');
    assert_eq!(coverage.incomplete[0].missing_characters[0].lines, [6, 7]);
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
