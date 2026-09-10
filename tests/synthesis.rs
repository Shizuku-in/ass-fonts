use ass_fonts::{
    FontFace, FontIndex, FontName, FontReference, MissingReason, NameKind, ResolveMode,
    ResolveOptions, SelectionMethod, SynthesisPolicy, WeightMatching,
};

fn face(path: &str, weight: u16, italic: bool) -> FontFace {
    FontFace {
        path: path.into(),
        face_index: 0,
        names: vec!["Example".into(), "Example-Regular".into()],
        family_names: vec!["Example".into()],
        name_records: vec![FontName {
            name: "Example-Regular".into(),
            kind: NameKind::PostScriptName,
            name_id: None,
        }],
        weight,
        italic,
    }
}

fn reference(name: &str, weight: u16, italic: bool) -> FontReference {
    FontReference {
        name: name.into(),
        weight,
        italic,
        lines: vec![1],
        characters: vec![],
    }
}

#[test]
fn strict_is_default_and_synthesis_is_opt_in() {
    let index = FontIndex::new([face("regular.ttf", 400, false)]);
    let refs = [reference("Example", 700, false)];
    let strict = index.resolve(&refs);
    assert_eq!(ResolveMode::default(), ResolveMode::Strict);
    assert_eq!(
        strict.missing[0].missing_reason,
        Some(MissingReason::StyleNotFound)
    );
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert_eq!(report.resolved.len(), 1);
    assert!(report.missing.is_empty());
    let result = &report.resolved[0];
    assert!(result.matches[0].synthetic_bold);
    assert_eq!(result.candidates[0].weight, 400);
    assert_eq!(result.reference.weight, 700);
    assert_eq!(result.missing_reason, None);
    assert!(result.available_variants.is_empty());
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["resolved"][0]["matches"][0]["synthetic_bold"], true);
}

#[test]
fn exact_faces_win_and_equal_fallbacks_remain_ambiguous() {
    let refs = [reference("Example", 700, false)];
    let normal = face("a.ttf", 400, false);
    let second = face("b.ttf", 400, false);
    let index = FontIndex::new([normal.clone(), second.clone(), face("bold.ttf", 700, false)]);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert_eq!(report.resolved.len(), 1);
    assert_eq!(report.resolved[0].candidates[0].weight, 700);
    assert!(!report.resolved[0].matches[0].synthetic_bold);

    let index = FontIndex::new([second, normal.clone(), normal]);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert_eq!(report.ambiguous.len(), 1);
    assert_eq!(report.ambiguous[0].candidates.len(), 2);
    assert!(report.ambiguous[0].matches.iter().all(|m| m.synthetic_bold));
    assert_eq!(
        report.ambiguous[0].candidates[0].path.to_str(),
        Some("a.ttf")
    );

    let index = FontIndex::new([
        face("a.ttf", 700, false),
        face("b.ttf", 700, false),
        face("c.ttf", 400, false),
    ]);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert_eq!(report.ambiguous[0].candidates.len(), 2);
    assert!(
        report.ambiguous[0]
            .matches
            .iter()
            .all(|m| !m.synthetic_bold)
    );
}

#[test]
fn bold_only_does_not_invent_weights_names_or_italics() {
    let index = FontIndex::new([
        face("regular.ttf", 400, false),
        face("light.ttf", 290, false),
    ]);
    let refs = [
        reference("Example", 600, false),
        reference("Example", 900, false),
        reference("Example", 700, true),
        reference("Absent", 700, false),
    ];
    let report = index.resolve_with_options(
        &refs,
        ResolveOptions {
            synthesis: SynthesisPolicy {
                bold: true,
                italic: false,
            },
            ..Default::default()
        },
    );
    assert_eq!(report.missing.len(), 4);
    assert_eq!(
        report.missing[3].missing_reason,
        Some(MissingReason::NameNotFound)
    );

    let light = FontIndex::new([face("light.ttf", 290, false)]);
    assert_eq!(
        light
            .resolve_with_mode(
                &[reference("Example", 700, false)],
                ResolveMode::AllowStyleSynthesis
            )
            .missing
            .len(),
        1
    );
    let italic = FontIndex::new([face("italic.ttf", 400, true)]);
    let report = italic.resolve_with_mode(
        &[reference("Example", 700, true)],
        ResolveMode::AllowStyleSynthesis,
    );
    assert!(report.resolved[0].matches[0].synthetic_bold);
    assert!(report.resolved[0].candidates[0].italic);
}

#[test]
fn italic_permission_and_native_priority() {
    let refs = [reference("Example", 400, true)];
    let upright = face("upright.ttf", 400, false);
    let index = FontIndex::new([upright.clone()]);
    assert_eq!(index.resolve(&refs).missing.len(), 1);
    let options = ResolveOptions {
        synthesis: SynthesisPolicy {
            bold: false,
            italic: true,
        },
        ..Default::default()
    };
    let report = index.resolve_with_options(&refs, options);
    assert_eq!(report.resolved.len(), 1);
    let m = &report.resolved[0].matches[0];
    assert!(m.synthetic_italic);
    assert!(!m.synthetic_bold);
    assert_eq!(m.selection_method, SelectionMethod::FamilySynthesis);
    assert!(!report.resolved[0].candidates[0].italic);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["resolved"][0]["matches"][0]["synthetic_italic"], true);
    let index = FontIndex::new([upright, face("native.ttf", 400, true)]);
    let report = index.resolve_with_options(&refs, options);
    assert!(report.resolved[0].candidates[0].italic);
    assert!(!report.resolved[0].matches[0].synthetic_italic);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilyExact
    );
}

#[test]
fn combined_synthesis_requires_both_permissions_under_exact_weight() {
    let index = FontIndex::new([face("normal.ttf", 400, false)]);
    let refs = [reference("Example", 700, true)];
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let report = index.resolve_with_options(
            &refs,
            ResolveOptions {
                synthesis: SynthesisPolicy { bold, italic },
                ..Default::default()
            },
        );
        if bold && italic {
            assert_eq!(report.resolved.len(), 1);
            assert!(report.resolved[0].matches[0].synthetic_bold);
            assert!(report.resolved[0].matches[0].synthetic_italic);
        } else {
            assert_eq!(report.missing.len(), 1);
        }
    }
    let index = FontIndex::new([face("native-bold.ttf", 700, false)]);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert!(!report.resolved[0].matches[0].synthetic_bold);
    assert!(report.resolved[0].matches[0].synthetic_italic);
}

#[test]
fn nearest_preserves_native_slant_before_upright_fallback() {
    let refs = [reference("Example", 700, true)];
    let options = ResolveOptions {
        weight_matching: WeightMatching::Nearest,
        synthesis: SynthesisPolicy {
            bold: true,
            italic: true,
        },
    };
    let index = FontIndex::new([
        face("upright.ttf", 700, false),
        face("italic.ttf", 693, true),
    ]);
    let report = index.resolve_with_options(&refs, options);
    assert_eq!(report.resolved[0].candidates[0].weight, 693);
    assert!(!report.resolved[0].matches[0].synthetic_italic);
    // Native italic + synthetic bold also precedes synthetic italic.
    let index = FontIndex::new([
        face("upright.ttf", 700, false),
        face("italic.ttf", 400, true),
    ]);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert!(report.resolved[0].matches[0].synthetic_bold);
    assert!(!report.resolved[0].matches[0].synthetic_italic);
    let index = FontIndex::new([face("upright.ttf", 693, false)]);
    let report = index.resolve_with_options(&refs, options);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::FamilyNearest
    );
    assert!(report.resolved[0].matches[0].synthetic_italic);
    assert!(!report.resolved[0].matches[0].synthetic_bold);
}

#[test]
fn italic_ties_remain_ambiguous_and_synthesis_is_not_reversed() {
    let first = face("a.ttc", 400, false);
    let mut second = first.clone();
    second.face_index = 1;
    let index = FontIndex::new([second, first.clone(), first]);
    let report = index.resolve_with_mode(
        &[reference("Example", 400, true)],
        ResolveMode::AllowStyleSynthesis,
    );
    assert_eq!(report.ambiguous[0].candidates.len(), 2);
    assert!(
        report.ambiguous[0]
            .matches
            .iter()
            .all(|m| m.synthetic_italic)
    );
    let index = FontIndex::new([face("italic.ttf", 400, true)]);
    let report = index.resolve_with_options(
        &[
            reference("Example", 400, false),
            reference("Missing", 400, true),
        ],
        ResolveOptions {
            weight_matching: WeightMatching::Nearest,
            synthesis: SynthesisPolicy {
                bold: true,
                italic: true,
            },
        },
    );
    assert_eq!(report.missing.len(), 2);
    assert_eq!(
        report.missing[0].missing_reason,
        Some(MissingReason::StyleNotFound)
    );
    assert_eq!(
        report.missing[1].missing_reason,
        Some(MissingReason::NameNotFound)
    );
}

#[test]
fn explicit_face_is_preserved_and_italic_requirement_is_reported() {
    let index = FontIndex::new([face("upright.ttf", 400, false)]);
    let refs = [reference("Example-Regular", 700, true)];
    let strict = index.resolve(&refs);
    assert_eq!(strict.resolved.len(), 1);
    assert!(!strict.resolved[0].matches[0].synthetic_italic);
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert_eq!(
        report.resolved[0].matches[0].selection_method,
        SelectionMethod::PostScriptName
    );
    assert!(report.resolved[0].matches[0].synthetic_italic);
    assert!(report.resolved[0].matches[0].synthetic_bold);
}

#[test]
fn explicit_names_keep_their_face_and_report_synthesis_per_candidate() {
    let index = FontIndex::new([face("regular.ttf", 400, false)]);
    let refs = [
        reference("Example-Regular", 700, false),
        reference("Example", 400, false),
    ];
    let strict = index.resolve(&refs);
    assert_eq!(strict.resolved.len(), 2);
    assert!(strict.resolved.iter().all(|r| !r.matches[0].synthetic_bold));
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
    assert!(report.resolved[0].matches[0].synthetic_bold);
    assert!(!report.resolved[1].matches[0].synthetic_bold);
    assert_eq!(
        report.resolved[0].matches[0].matched_names[0].kind,
        NameKind::PostScriptName
    );
}
