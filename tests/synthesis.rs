use ass_fonts::{
    FontFace, FontIndex, FontName, FontReference, MissingReason, NameKind, ResolveMode,
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
fn synthesis_does_not_invent_weights_names_or_italics() {
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
    let report = index.resolve_with_mode(&refs, ResolveMode::AllowStyleSynthesis);
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
