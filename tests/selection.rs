use ass_fonts::{
    FontFace, FontIndex, FontName, FontReference, NameKind, ResolveMode, ResolveOptions,
    SelectionMethod, SynthesisPolicy, WeightMatching,
};

fn record(name: &str, id: u16) -> FontName {
    FontName {
        name: name.into(),
        name_id: Some(id),
        kind: match id {
            1 | 16 | 21 => NameKind::Family,
            4 | 18 => NameKind::FullName,
            6 => NameKind::PostScriptName,
            _ => unreachable!(),
        },
    }
}

fn face(path: &str, weight: u16, records: Vec<FontName>) -> FontFace {
    FontFace {
        path: path.into(),
        face_index: 0,
        weight,
        italic: false,
        names: vec![],
        family_names: vec![],
        name_records: records,
    }
}

fn request(name: &str, weight: u16) -> FontReference {
    FontReference {
        name: name.into(),
        weight,
        italic: false,
        lines: vec![1],
        characters: vec![],
    }
}

#[test]
fn full_name_distinguishes_legacy_variant_from_typographic_family() {
    // Names are deliberately arbitrary; the resolver must not parse suffixes.
    let index = FontIndex::new([face(
        "variant.ttc",
        650,
        vec![
            record("Special", 1),
            record("Special", 4),
            record("Common", 16),
        ],
    )]);
    let report = index.resolve(&[request("@SPECIAL", 400), request("Common", 400)]);
    assert_eq!(report.resolved.len(), 1);
    assert_eq!(report.resolved[0].candidates[0].weight, 650);
    let evidence = &report.resolved[0].matches[0];
    assert_eq!(evidence.selection_method, SelectionMethod::FullName);
    assert_eq!(evidence.matched_names[0].name_id, Some(4));
    assert!(!evidence.synthetic_bold);
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].reference.name, "Common");
}

#[test]
fn generic_full_name_does_not_pin_regular_and_ps_has_priority() {
    let regular = face(
        "regular.ttf",
        400,
        vec![
            record("Common", 1),
            record("Common", 4),
            record("Common", 16),
            record("Chosen", 6),
        ],
    );
    let bold = face(
        "bold.ttf",
        700,
        vec![
            record("Common", 1),
            record("Heavy", 4),
            record("Common", 16),
            record("Chosen", 1),
        ],
    );
    let index = FontIndex::new([regular, bold]);
    let report = index.resolve(&[request("Common", 700), request("Chosen", 700)]);
    assert_eq!(report.resolved.len(), 2);
    assert_eq!(report.resolved[0].candidates[0].weight, 700);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilyExact
    );
    assert_eq!(report.resolved[1].candidates[0].weight, 400);
    assert_eq!(
        report.resolved[1].matches[0].selection_method,
        SelectionMethod::PostScriptName
    );
}

#[test]
fn duplicate_variants_stay_ambiguous_and_input_order_is_stable() {
    let first = face(
        "a.ttc",
        900,
        vec![
            record("Variant", 1),
            record("Variant", 4),
            record("Common", 16),
        ],
    );
    let mut second = first.clone();
    second.path = "b.otf".into();
    let refs = [request("Variant", 400)];
    let a = FontIndex::new([second.clone(), first.clone(), first.clone()]).resolve(&refs);
    let b = FontIndex::new([first, second]).resolve(&refs);
    assert_eq!(a.ambiguous.len(), 1);
    assert_eq!(a.ambiguous[0].candidates.len(), 2);
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

#[test]
fn incomplete_or_conflicting_family_metadata_is_conservative() {
    let legacy = face(
        "legacy.ttf",
        400,
        vec![record("Common", 1), record("Common", 4)],
    );
    assert_eq!(
        FontIndex::new([legacy])
            .resolve(&[request("Common", 700)])
            .missing
            .len(),
        1
    );
    let specific = face(
        "one.ttf",
        650,
        vec![
            record("Special", 1),
            record("Special", 4),
            record("Common", 16),
        ],
    );
    let conflicting = face(
        "two.ttf",
        700,
        vec![record("Special", 16), record("Another", 4)],
    );
    let report = FontIndex::new([specific, conflicting]).resolve(&[request("Special", 700)]);
    assert_eq!(report.resolved[0].candidates[0].weight, 700);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilyExact
    );
}

#[test]
fn options_and_compatibility_mode_agree() {
    let index = FontIndex::new([face("regular.ttf", 400, vec![record("Common", 1)])]);
    let refs = [request("Common", 700)];
    let options = ResolveOptions {
        weight_matching: WeightMatching::Exact,
        synthesis: SynthesisPolicy {
            bold: true,
            italic: true,
        },
    };
    let report = index.resolve_with_options(&refs, options);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilySynthesis
    );
    assert!(report.resolved[0].matches[0].synthetic_bold);
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        serde_json::to_value(index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis))
            .unwrap()
    );
    assert_eq!(
        index
            .resolve_with_options(&refs, ResolveOptions::default())
            .missing
            .len(),
        1
    );
}

#[test]
fn legacy_variant_alias_can_differ_from_full_name() {
    let records = vec![
        record("Distinct alias", 1),
        record("Different full spelling", 4),
        record("Common", 16),
    ];
    let index = FontIndex::new([face("black.ttf", 900, records.clone())]);
    let report = index.resolve(&[request("Distinct alias", 400)]);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::LegacyFamilyName
    );
    assert_eq!(
        report.resolved[0].matches[0].matched_names[0].name_id,
        Some(1)
    );
    // A legacy group containing different styles must still select by weight.
    let index = FontIndex::new([
        face("black.ttf", 900, records.clone()),
        face("normal.ttf", 400, records),
    ]);
    let report = index.resolve(&[request("Distinct alias", 400)]);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilyExact
    );
    assert_eq!(report.resolved[0].candidates[0].weight, 400);
}
