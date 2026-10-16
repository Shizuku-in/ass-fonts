#![no_main]

use ass_fonts::{extract_fonts, normalize_name, rewrite_font_names};
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    assert_eq!(rewrite_font_names(text, |_| None).text, text);

    // A consistent renaming must extract the same references under new names.
    let mut mapping = BTreeMap::new();
    let rewritten = rewrite_font_names(text, |site| {
        let next = mapping.len();
        let name = mapping
            .entry(normalize_name(site.name))
            .or_insert_with(|| format!("F{next}"));
        Some(name.clone())
    });
    let before = extract_fonts(text);
    let after = extract_fonts(&rewritten.text);
    assert_eq!(before.diagnostics, after.diagnostics);
    let key = |name: &str, weight: u16, italic: bool| (normalize_name(name), weight, italic);
    let mut expected: Vec<_> = before
        .references
        .into_iter()
        .map(|mut reference| {
            reference.name = mapping[&normalize_name(&reference.name)].clone();
            reference
        })
        .collect();
    let mut actual = after.references;
    expected.sort_by_key(|r| key(&r.name, r.weight, r.italic));
    actual.sort_by_key(|r| key(&r.name, r.weight, r.italic));
    assert_eq!(expected.len(), actual.len());
    for (expected, actual) in expected.iter().zip(&actual) {
        assert_eq!(
            key(&expected.name, expected.weight, expected.italic),
            key(&actual.name, actual.weight, actual.italic)
        );
        assert_eq!(
            (
                &expected.lines,
                &expected.characters,
                &expected.whitespace,
                expected.vertical
            ),
            (
                &actual.lines,
                &actual.characters,
                &actual.whitespace,
                actual.vertical
            )
        );
    }
});
