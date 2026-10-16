use ass_fonts::{SiteKind, extract_fonts, normalize_name, rewrite_font_names};
use proptest::prelude::*;
use std::collections::BTreeMap;

const SOURCE: &str = "\u{feff}[Script Info]\r\n; {\\fnInComment}\n\
[V4+ Styles]\r\n\
Format: Name, Fontname, Bold\r\n\
Style: Default, @Foo ,0\n\
Style: Unused,Never,1\r\n\
Style: Broken,Foo\n\
[Events]\n\
Format: Style, Text\r\n\
Dialogue: Default,{\\fn Foo \\b1\\t(\\fnFoo)}x{\\fn}y{\\fnBar\r\n\
Comment: Default,{\\fn@Bar}hidden\n\
[Fonts]\n\
fontname: Foo.ttf\n";

fn rename(name: &str) -> Option<String> {
    match name {
        "Foo" => Some("F1".into()),
        "Never" => Some("F2".into()),
        "Bar" => Some("F3".into()),
        _ => None,
    }
}

#[test]
fn rewrites_every_site_and_preserves_other_bytes() {
    let mut sites = vec![];
    let rewritten = rewrite_font_names(SOURCE, |site| {
        sites.push((
            site.line,
            site.name.to_owned(),
            site.vertical,
            site.kind == SiteKind::OverrideFn,
        ));
        rename(site.name)
    });
    let expected = SOURCE
        .replace("Style: Default, @Foo ,0", "Style: Default, @F1 ,0")
        .replace("Unused,Never", "Unused,F2")
        .replace("{\\fn Foo \\b1", "{\\fn F1 \\b1")
        .replace("{\\fn@Bar}", "{\\fn@F3}");
    assert_eq!(rewritten.text, expected);
    assert_eq!(
        sites,
        [
            (5, "Foo".into(), true, false),
            (6, "Never".into(), false, false),
            (10, "Foo".into(), false, true),
            (11, "Bar".into(), true, true),
        ]
    );
    // The malformed style and the unclosed block are reported, not rewritten.
    let lines: Vec<_> = rewritten.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(lines, [7, 10]);
}

#[test]
fn identity_and_invalid_replacements_keep_the_source() {
    assert_eq!(rewrite_font_names(SOURCE, |_| None).text, SOURCE);
    for invalid in ["", " ", " F", "@F", "a\\b", "a{b", "a}b", "a(b", "a\nb"] {
        let rewritten = rewrite_font_names(SOURCE, |site| {
            (site.kind == SiteKind::OverrideFn).then(|| invalid.to_owned())
        });
        assert_eq!(rewritten.text, SOURCE, "{invalid:?}");
        assert_eq!(rewritten.diagnostics.len(), 4, "{invalid:?}");
    }
    // A comma is harmless inside an override tag.
    let rewritten = rewrite_font_names(SOURCE, |site| {
        (site.kind == SiteKind::OverrideFn).then(|| "a,b".to_owned())
    });
    assert!(rewritten.text.contains("{\\fn a,b \\b1"));
}

#[test]
fn rewritten_names_extract_to_the_same_references() {
    let rewritten = rewrite_font_names(SOURCE, |site| rename(site.name));
    let references = extract_fonts(&rewritten.text).references;
    // `x` uses `\fn F1` and `y` resets to the vertical style font `@F1`.
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].name, "F1");
    assert_eq!(references[0].weight, 700);
    assert!(references[0].vertical);
}

const NAMES: &[&str] = &[
    "Foo",
    "@Foo",
    " foo ",
    "@ Bar",
    "宋体",
    "",
    "@",
    "A(b",
    "x\\y",
    "Ｆｏｏ",
    "a,b",
];
const STYLES: &[&str] = &["Default", "Alt", "Missing", ""];

fn text_piece(kind: usize, name: &str, style: &str) -> String {
    match kind {
        0 => format!("{{\\fn{name}}}"),
        1 => format!("{{\\fn{name}\\b1}}"),
        2 => format!("{{\\t(\\fn{name})\\fn{name}}}"),
        3 => format!("{{\\r{style}}}"),
        4 => "{\\i1\\b0}".into(),
        5 => "{\\fn}".into(),
        6 => "{\\p1}m 0 0{\\p0}".into(),
        7 => "{unclosed\\fnLost".into(),
        8 => "字 x".into(),
        9 => "\\h\\N\\n　".into(),
        _ => " , ".into(),
    }
}

fn line(kind: usize, name: &str, style: &str, text: &str) -> String {
    match kind {
        0 => "[V4+ Styles]".into(),
        1 => "[V4 Styles]".into(),
        2 => "[Events]".into(),
        3 => "[Fonts]".into(),
        4 => "Format: Name, Fontname, Bold".into(),
        5 => "Format: Fontname,Name".into(),
        6 => format!("Style: {style},{name},1"),
        7 => format!("Style: {name},{style}"),
        8 => "Format: Style, Text".into(),
        9 => format!("Dialogue: {style},{text}"),
        10 => format!("Comment: {style},{text}"),
        11 => format!("; {text}"),
        12 => format!("fontname: {name}"),
        _ => String::new(),
    }
}

fn subtitle() -> impl Strategy<Value = String> {
    let piece = (0..11usize, 0..NAMES.len(), 0..STYLES.len());
    let record = (
        0..14usize,
        0..NAMES.len(),
        0..STYLES.len(),
        prop::collection::vec(piece, 0..6),
        0..3usize,
    );
    (any::<bool>(), prop::collection::vec(record, 0..24)).prop_map(|(bom, records)| {
        let mut text = if bom {
            "\u{feff}".to_owned()
        } else {
            String::new()
        };
        for (kind, name, style, pieces, ending) in records {
            let body: String = pieces
                .into_iter()
                .map(|(kind, name, style)| text_piece(kind, NAMES[name], STYLES[style]))
                .collect();
            text += &line(kind, NAMES[name], STYLES[style], &body);
            text += ["\n", "\r\n", " \r\n"][ending];
        }
        text
    })
}

proptest! {
    #[test]
    fn keeping_every_name_is_byte_identical(text in subtitle()) {
        prop_assert_eq!(rewrite_font_names(&text, |_| None).text, text);
    }

    #[test]
    fn consistent_renaming_preserves_extraction(text in subtitle()) {
        let mut mapping = BTreeMap::new();
        let rewritten = rewrite_font_names(&text, |site| {
            let next = mapping.len();
            Some(mapping.entry(normalize_name(site.name)).or_insert_with(|| format!("F{next}")).clone())
        });
        prop_assert!(rewritten.diagnostics.iter().all(|d| !d.message.contains("invalid replacement")));
        let before = extract_fonts(&text);
        let after = extract_fonts(&rewritten.text);
        prop_assert_eq!(&before.diagnostics, &after.diagnostics);
        let mut expected: Vec<_> = before
            .references
            .into_iter()
            .map(|mut reference| {
                reference.name = normalize_name(&mapping[&normalize_name(&reference.name)]);
                reference
            })
            .collect();
        let mut actual: Vec<_> = after
            .references
            .into_iter()
            .map(|mut reference| {
                reference.name = normalize_name(&reference.name);
                reference
            })
            .collect();
        expected.sort_by(|a, b| (&a.name, a.weight, a.italic).cmp(&(&b.name, b.weight, b.italic)));
        actual.sort_by(|a, b| (&a.name, a.weight, a.italic).cmp(&(&b.name, b.weight, b.italic)));
        prop_assert_eq!(expected, actual);
    }
}
