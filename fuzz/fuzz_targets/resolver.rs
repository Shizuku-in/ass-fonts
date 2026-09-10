#![no_main]

use arbitrary::Arbitrary;
use ass_fonts::{
    CharacterUsage, FontFace, FontIndex, FontName, FontReference, NameKind, ResolveOptions,
    SynthesisPolicy, WeightMatching,
};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    faces: Vec<FaceInput>,
    references: Vec<ReferenceInput>,
    nearest: bool,
    synthetic_bold: bool,
    synthetic_italic: bool,
}

#[derive(Debug, Arbitrary)]
struct FaceInput {
    path: u8,
    face_index: u8,
    names: Vec<String>,
    families: Vec<String>,
    records: Vec<NameInput>,
    weight: u16,
    italic: bool,
}

#[derive(Debug, Arbitrary)]
struct NameInput {
    name: String,
    kind: u8,
}

#[derive(Debug, Arbitrary)]
struct ReferenceInput {
    name: String,
    weight: u16,
    italic: bool,
    character: char,
}

fn text(value: String) -> String {
    value.chars().take(128).collect()
}

fuzz_target!(|input: Input| {
    let faces = input.faces.into_iter().take(64).map(|face| FontFace {
        path: format!("font-{}.ttc", face.path % 16).into(),
        face_index: u32::from(face.face_index % 8),
        names: face.names.into_iter().take(8).map(text).collect(),
        family_names: face.families.into_iter().take(8).map(text).collect(),
        name_records: face
            .records
            .into_iter()
            .take(8)
            .map(|record| {
                let (kind, name_id) = match record.kind % 4 {
                    0 => (NameKind::Family, Some(16)),
                    1 => (NameKind::FullName, Some(4)),
                    2 => (NameKind::PostScriptName, Some(6)),
                    _ => (NameKind::InternalName, None),
                };
                FontName {
                    name: text(record.name),
                    kind,
                    name_id,
                }
            })
            .collect(),
        weight: face.weight,
        italic: face.italic,
    });
    let references = input
        .references
        .into_iter()
        .take(64)
        .enumerate()
        .map(|(index, reference)| FontReference {
            name: text(reference.name),
            weight: reference.weight,
            italic: reference.italic,
            lines: vec![index + 1],
            characters: vec![CharacterUsage {
                character: reference.character,
                lines: vec![index + 1],
            }],
        })
        .collect::<Vec<_>>();
    let index = FontIndex::new(faces);
    let _ = index.resolve_with_options(
        &references,
        ResolveOptions {
            weight_matching: if input.nearest {
                WeightMatching::Nearest
            } else {
                WeightMatching::Exact
            },
            synthesis: SynthesisPolicy {
                bold: input.synthetic_bold,
                italic: input.synthetic_italic,
            },
        },
    );
});
