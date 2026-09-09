use ass_fonts::{FontFace, FontIndex, FontReference, ScanReport, extract_fonts, read_subtitle};

fn script(text: &str) -> String {
    format!(
        "[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Base\nStyle: Alt,Other\nStyle: Unused,Never\n[Events]\nFormat: Style, Text\nDialogue: Default,{text}\n"
    )
}

fn names(text: &str) -> Vec<String> {
    extract_fonts(text)
        .references
        .into_iter()
        .map(|r| r.name)
        .collect()
}

#[test]
fn actual_runs_resets_and_drawing() {
    assert_eq!(
        names(&script(
            r"{\fnInline}Hello, world{\rAlt}字{\fn}文{\r}base{\p1\fnDrawing}m 0 0 l 1 1{\p0\fnLast}end"
        )),
        ["Base", "Inline", "Last", "Other"]
    );
    assert_eq!(
        names(&script(r"{\fnFirst\fnSecond}x{\fnTrailing}")),
        ["Second"]
    );
    assert!(names(&script(r" \N\n\h{\p1}m 0 0")).is_empty());
    assert_eq!(
        names(&script(r"{\t(0,100,\fnNotAnimated)\fnActual}x")),
        ["Actual"]
    );
}

#[test]
fn ssa_reordered_fields_comments_and_late_styles() {
    let source = "\u{feff}[Events]\nFormat: Start, Style, Text\nComment: 0,Default,ignored\nDialogue: 0,Default,hello,comma\n[V4 Styles]\nFormat: Fontname,Name\nStyle: 中文字体,Default\n";
    let parsed = extract_fonts(source);
    assert_eq!(parsed.references[0].name, "中文字体");
    assert_eq!(parsed.references[0].lines, [4]);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn malformed_records_and_missing_styles_are_diagnosed() {
    let source = "[V4+ Styles]\nFormat: Name,Fontname\nStyle: broken\n[Events]\nFormat: Style,Text\nDialogue: Unknown,text\nDialogue: broken\n";
    let parsed = extract_fonts(source);
    assert_eq!(parsed.diagnostics.len(), 3);
    assert!(parsed.references.is_empty());
    assert!(!extract_fonts(&script("{unclosed")).diagnostics.is_empty());
}

#[test]
fn normalization_aliases_deduplication_and_ambiguity() {
    let face = |path: &str, index, names: &[&str]| FontFace {
        path: path.into(),
        face_index: index,
        names: names.iter().map(|s| s.to_string()).collect(),
    };
    let first = face("a.ttc", 0, &["Example", "Example-Regular", "中文"]);
    let index = FontIndex::new([first.clone(), first, face("a.ttc", 1, &["Example"])]);
    let references: Vec<_> = [" @ＥＸＡＭＰＬＥ ", "example-regular", "Absent", "中文"]
        .into_iter()
        .map(|name| FontReference {
            name: name.into(),
            lines: vec![1],
        })
        .collect();
    let report = index.resolve(&references);
    assert_eq!(report.resolved.len(), 2);
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.ambiguous.len(), 1);
    assert_eq!(report.ambiguous[0].candidates.len(), 2);
    assert!(
        serde_json::to_string(&report)
            .unwrap()
            .contains("ambiguous")
    );
}

// Minimal synthetic SFNTs avoid system-font dependencies and font redistribution.
fn put16(bytes: &mut [u8], offset: usize, n: u16) {
    bytes[offset..offset + 2].copy_from_slice(&n.to_be_bytes());
}
fn put32(bytes: &mut [u8], offset: usize, n: u32) {
    bytes[offset..offset + 4].copy_from_slice(&n.to_be_bytes());
}

fn sfnt(name: &str, base: usize, otf: bool) -> Vec<u8> {
    let utf16: Vec<u8> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let mut naming = vec![0; 18];
    put16(&mut naming, 2, 1);
    put16(&mut naming, 4, 18);
    put16(&mut naming, 6, 3); // Windows Unicode BMP
    put16(&mut naming, 8, 1);
    put16(&mut naming, 10, 0x409);
    put16(&mut naming, 12, 1);
    put16(&mut naming, 14, utf16.len() as u16);
    naming.extend(utf16);
    let mut head = vec![0; 54];
    put16(&mut head, 18, 1000);
    let mut hhea = vec![0; 36];
    put32(&mut hhea, 0, 0x10000);
    let mut maxp = vec![0; 6];
    put32(&mut maxp, 0, 0x5000);
    put16(&mut maxp, 4, 1);
    let tables = [
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"name", naming),
    ];
    let mut bytes = vec![0; 12 + 16 * tables.len()];
    put32(
        &mut bytes,
        0,
        if otf {
            u32::from_be_bytes(*b"OTTO")
        } else {
            0x10000
        },
    );
    put16(&mut bytes, 4, tables.len() as u16);
    for (i, (tag, data)) in tables.into_iter().enumerate() {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let offset = bytes.len();
        let record = 12 + i * 16;
        bytes[record..record + 4].copy_from_slice(&tag);
        put32(&mut bytes, record + 8, (base + offset) as u32);
        put32(&mut bytes, record + 12, data.len() as u32);
        bytes.extend(data);
    }
    bytes
}

fn collection(otf: bool) -> Vec<u8> {
    let mut bytes = vec![0; 20];
    bytes[..4].copy_from_slice(b"ttcf");
    put32(&mut bytes, 4, 0x10000);
    put32(&mut bytes, 8, 2);
    put32(&mut bytes, 12, 20);
    bytes.extend(sfnt("Collection One", 20, otf));
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let offset = bytes.len();
    put32(&mut bytes, 16, offset as u32);
    bytes.extend(sfnt("Collection Two", offset, otf));
    bytes
}

#[test]
fn scans_all_formats_and_reports_corruption() {
    let dir = tempfile::tempdir().unwrap();
    for (file, data) in [
        ("misleading.TTF", sfnt("Internal Name", 0, false)),
        ("one.otf", sfnt("OpenType", 0, true)),
        ("two.ttc", collection(false)),
        ("three.otc", collection(true)),
        ("bad.ttf", vec![1, 2, 3]),
    ] {
        std::fs::write(dir.path().join(file), data).unwrap();
    }
    let scan = ScanReport::scan([dir.path(), dir.path()]);
    assert_eq!(scan.faces.len(), 6, "{:#?}", scan.issues);
    assert_eq!(scan.issues.len(), 1);
    assert_eq!(scan.faces.iter().filter(|f| f.face_index == 1).count(), 2);
    let refs = extract_fonts(&script(r"{\fnInternal Name}x{\fnmisleading}y"));
    let report = FontIndex::new(scan.faces).resolve(&refs.references);
    assert_eq!(report.resolved.len(), 1);
    assert_eq!(report.missing.len(), 1);
}

#[test]
fn unicode_file_decoding_is_strict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subtitle.ssa");
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        bytes.extend(script("文字").encode_utf16().flat_map(|u| {
            if little {
                u.to_le_bytes()
            } else {
                u.to_be_bytes()
            }
        }));
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(read_subtitle(&path).unwrap().references[0].name, "Base");
    }
    std::fs::write(&path, [0xff, 0xfe, 0]).unwrap();
    assert!(read_subtitle(&path).is_err());
    std::fs::write(&path, [0xff]).unwrap();
    assert!(read_subtitle(&path).is_err());
}
