use ass_fonts::{
    CharacterUsage, FontFace, FontIndex, FontName, FontReference, NameKind, ResolveOptions,
    WeightMatching, extract_fonts,
};
use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};

const DIALOGUES: usize = 5_000;
const FACES: usize = 2_000;

fn subtitle(dialogues: usize) -> String {
    let mut text = String::from(
        "[V4+ Styles]\nFormat: Name,Fontname,Bold,Italic\n\
         Style: Default,Example Sans,0,0\n\
         Style: Alternate,Example Serif,-1,-1\n\
         [Events]\nFormat: Style,Text\n",
    );
    for index in 0..dialogues {
        text.push_str("Dialogue: Default,Line ");
        text.push_str(&index.to_string());
        text.push_str(" 中文{\\b1}Bold{\\i1}Italic{\\rAlternate}替代{\\p1}m 0 0 l 1 1{\\p0}End\n");
    }
    text
}

fn faces(count: usize) -> Vec<FontFace> {
    (0..count)
        .map(|index| {
            let family = format!("Benchmark Family {index}");
            FontFace {
                path: format!("fonts/{index}.ttf").into(),
                face_index: 0,
                names: vec![family.clone()],
                family_names: vec![family.clone()],
                name_records: vec![FontName {
                    name: family,
                    kind: NameKind::Family,
                    name_id: Some(16),
                }],
                weight: if index % 2 == 0 { 400 } else { 700 },
                italic: index % 4 == 3,
            }
        })
        .collect()
}

fn references(count: usize, nearest: bool) -> Vec<FontReference> {
    (0..count)
        .map(|index| FontReference {
            name: format!("Benchmark Family {index}"),
            weight: if nearest {
                650
            } else if index % 2 == 0 {
                400
            } else {
                700
            },
            italic: index % 4 == 3,
            lines: vec![index + 1],
            characters: vec![CharacterUsage {
                character: 'A',
                lines: vec![index + 1],
            }],
        })
        .collect()
}

fn extraction(c: &mut Criterion) {
    let source = subtitle(DIALOGUES);
    let mut group = c.benchmark_group("subtitle");
    group.throughput(Throughput::Elements(DIALOGUES as u64));
    group.bench_function("extract_5000_dialogues", |b| {
        b.iter(|| extract_fonts(black_box(&source)))
    });
    group.finish();
}

fn indexing(c: &mut Criterion) {
    let input = faces(FACES);
    let mut group = c.benchmark_group("font_index");
    group.throughput(Throughput::Elements(FACES as u64));
    group.bench_function("build_2000_faces", |b| {
        b.iter_batched(
            || input.clone(),
            |faces| FontIndex::new(black_box(faces)),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn resolution(c: &mut Criterion) {
    let index = FontIndex::new(faces(FACES));
    let exact = references(FACES, false);
    let nearest = references(FACES, true);
    let mut group = c.benchmark_group("resolution");
    group.throughput(Throughput::Elements(FACES as u64));
    group.bench_function("exact_2000_references", |b| {
        b.iter(|| index.resolve(black_box(&exact)))
    });
    group.bench_function("nearest_2000_references", |b| {
        b.iter(|| {
            index.resolve_with_options(
                black_box(&nearest),
                ResolveOptions {
                    weight_matching: WeightMatching::Nearest,
                    ..Default::default()
                },
            )
        })
    });
    group.finish();
}

criterion_group!(benches, extraction, indexing, resolution);
criterion_main!(benches);
