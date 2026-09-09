use ass_fonts::{
    FontFace, FontIndex, FontName, FontReference, MissingReason, NameKind, ResolveOptions,
    SelectionMethod, SynthesisPolicy, WeightMatching,
};

fn face(path: &str, weight: u16, italic: bool) -> FontFace {
    FontFace {
        path: path.into(),
        face_index: 0,
        weight,
        italic,
        names: vec!["Family".into()],
        family_names: vec!["Family".into()],
        name_records: vec![],
    }
}

fn request(weight: u16, italic: bool) -> FontReference {
    FontReference {
        name: "Family".into(),
        weight,
        italic,
        lines: vec![1],
    }
}

fn options(bold: bool) -> ResolveOptions {
    ResolveOptions {
        weight_matching: WeightMatching::Nearest,
        synthesis: SynthesisPolicy { bold },
    }
}

#[test]
fn nearest_is_opt_in_and_reports_actual_weight_without_synthesis() {
    let index = FontIndex::new([
        face("398.ttf", 398, false),
        face("420.ttf", 420, false),
        face("693.ttf", 693, false),
        face("721.ttf", 721, false),
    ]);
    let refs = [request(400, false), request(700, false)];
    assert_eq!(index.resolve(&refs).missing.len(), 2);
    for bold in [false, true] {
        let report = index.resolve_with_options(&refs, options(bold));
        assert_eq!(report.resolved.len(), 2);
        assert!(report.missing.is_empty());
        for (result, expected) in report.resolved.iter().zip([398, 693]) {
            assert_eq!(result.candidates[0].weight, expected);
            assert_eq!(
                result.matches[0].selection_method,
                SelectionMethod::FamilyNearest
            );
            assert!(!result.matches[0].synthetic_bold);
            assert_eq!(result.matches[0].matched_names[0].kind, NameKind::Family);
        }
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(
            json["resolved"][0]["matches"][0]["selection_method"],
            "family_nearest"
        );
        assert_eq!(json["resolved"][0]["reference"]["weight"], 400);
        assert_eq!(json["resolved"][0]["candidates"][0]["weight"], 398);
    }
    let only_medium = FontIndex::new([face("medium.ttf", 500, false)]);
    assert_eq!(
        only_medium
            .resolve_with_options(&[request(400, false)], options(false))
            .resolved[0]
            .candidates[0]
            .weight,
        500
    );
}

#[test]
fn exact_matches_win_and_equal_distances_remain_ambiguous() {
    let low = face("a.ttf", 350, false);
    let high = face("b.ttf", 450, false);
    let mut duplicate = high.clone();
    duplicate.face_index = 1;
    let refs = [request(400, false)];
    let index = FontIndex::new([high.clone(), low.clone(), duplicate.clone(), low.clone()]);
    let report = index.resolve_with_options(&refs, options(false));
    assert_eq!(report.ambiguous.len(), 1);
    assert_eq!(report.ambiguous[0].candidates.len(), 3);
    assert!(
        report.ambiguous[0]
            .matches
            .iter()
            .all(|m| m.selection_method == SelectionMethod::FamilyNearest)
    );
    let reordered = FontIndex::new([duplicate, low.clone(), high.clone()])
        .resolve_with_options(&refs, options(false));
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(reordered).unwrap()
    );
    let index = FontIndex::new([
        low,
        high,
        face("exact.ttf", 400, false),
        face("exact-copy.ttf", 400, false),
    ]);
    let report = index.resolve_with_options(&refs, options(true));
    assert_eq!(report.ambiguous[0].candidates.len(), 2);
    assert!(
        report.ambiguous[0]
            .matches
            .iter()
            .all(|m| m.selection_method == SelectionMethod::FamilyExact)
    );
}

#[test]
fn nearest_requires_same_family_and_italic_state() {
    let index = FontIndex::new([
        face("upright.ttf", 400, false),
        face("italic.ttf", 500, true),
    ]);
    let report = index.resolve_with_options(&[request(400, true)], options(true));
    assert_eq!(report.resolved[0].candidates[0].weight, 500);
    assert!(report.resolved[0].candidates[0].italic);
    let index = FontIndex::new([face("upright.ttf", 400, false)]);
    let mut absent = request(700, false);
    absent.name = "Absent".into();
    let report = index.resolve_with_options(&[request(700, true), absent], options(true));
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
fn nearest_and_synthesis_are_independent_and_specific_names_stay_specific() {
    let mut normal = face("regular.ttf", 400, false);
    normal.name_records.push(FontName {
        name: "Regular-PS".into(),
        kind: NameKind::PostScriptName,
        name_id: Some(6),
    });
    let index = FontIndex::new([normal.clone()]);
    let refs = [request(700, false)];
    for bold in [false, true] {
        let report = index.resolve_with_options(&refs, options(bold));
        assert_eq!(
            report.resolved[0].matches[0].selection_method,
            SelectionMethod::FamilyNearest
        );
        assert_eq!(report.resolved[0].matches[0].synthetic_bold, bold);
    }
    let mut specific = request(700, false);
    specific.name = "Regular-PS".into();
    let index = FontIndex::new([normal, face("closer.ttf", 693, false)]);
    let report = index.resolve_with_options(&[request(700, false), specific], options(true));
    assert_eq!(report.resolved[0].candidates[0].weight, 693);
    assert!(!report.resolved[0].matches[0].synthetic_bold);
    assert_eq!(report.resolved[1].candidates[0].weight, 400);
    assert_eq!(
        report.resolved[1].matches[0].selection_method,
        SelectionMethod::PostScriptName
    );
    assert!(report.resolved[1].matches[0].synthetic_bold);
}
