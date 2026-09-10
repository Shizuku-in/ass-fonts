use ass_fonts::{
    CharacterUsage, FontFace, FontIndex, FontName, FontReference, NameKind, ResolveOptions,
    ResolveReport, ScanReport, WeightMatching, extract_fonts,
};
use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use tempfile::TempDir;

const DIALOGUES: usize = 5_000;
const FACES: usize = 2_000;
const SCAN_FILES: usize = 100;
const COVERAGE_REFERENCES: usize = 100;
const COVERAGE_CHARACTERS: usize = 512;
const CHARACTERS_PER_REFERENCE: usize = 128;

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

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

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

fn cjk_characters() -> Vec<char> {
    (0..COVERAGE_CHARACTERS)
        .map(|offset| char::from_u32(0x4e00 + offset as u32).unwrap())
        .collect()
}

fn font_directory(count: usize, characters: &[char]) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..count {
        std::fs::write(
            directory.path().join(format!("font-{index}.ttf")),
            font(&format!("Benchmark Coverage {index}"), characters),
        )
        .unwrap();
    }
    directory
}

fn character_usage(characters: &[char], reference: usize) -> Vec<CharacterUsage> {
    let mut selected = (0..CHARACTERS_PER_REFERENCE)
        .map(|offset| characters[(reference * 17 + offset) % characters.len()])
        .collect::<Vec<_>>();
    selected.sort_unstable();
    selected.dedup();
    selected
        .into_iter()
        .map(|character| CharacterUsage {
            character,
            lines: vec![reference + 1],
        })
        .collect()
}

fn coverage_fixture(face_count: usize, references_per_face: usize) -> (TempDir, ResolveReport) {
    let characters = cjk_characters();
    let directory = font_directory(face_count, &characters);
    let scan = ScanReport::scan([directory.path()]);
    assert!(scan.issues.is_empty());
    assert_eq!(scan.faces.len(), face_count);
    let references = (0..face_count)
        .flat_map(|face| {
            let characters = &characters;
            (0..references_per_face).map(move |reference| FontReference {
                name: format!("Benchmark Coverage {face}"),
                weight: 400,
                italic: false,
                lines: vec![reference + 1],
                characters: character_usage(characters, reference),
            })
        })
        .collect::<Vec<_>>();
    let report = FontIndex::new(scan.faces).resolve(&references);
    assert_eq!(report.resolved.len(), face_count * references_per_face);
    (directory, report)
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

fn scanning(c: &mut Criterion) {
    let characters = cjk_characters();
    let directory = font_directory(SCAN_FILES, &characters);
    let warmup = ScanReport::scan([directory.path()]);
    assert!(warmup.issues.is_empty());
    assert_eq!(warmup.faces.len(), SCAN_FILES);

    let mut group = c.benchmark_group("font_scan");
    group.throughput(Throughput::Elements(SCAN_FILES as u64));
    group.bench_function("warm_100_ttf", |b| {
        b.iter(|| ScanReport::scan([black_box(directory.path())]))
    });
    group.finish();
}

fn coverage(c: &mut Criterion) {
    let (_shared_directory, shared) = coverage_fixture(1, COVERAGE_REFERENCES);
    let (_ten_face_directory, ten_faces) = coverage_fixture(10, COVERAGE_REFERENCES / 10);
    let shared_check = shared.check_coverage();
    let ten_face_check = ten_faces.check_coverage();
    assert!(shared_check.is_complete());
    assert!(ten_face_check.is_complete());
    assert_eq!(shared_check.complete.len(), COVERAGE_REFERENCES);
    assert_eq!(ten_face_check.complete.len(), COVERAGE_REFERENCES);
    assert_eq!(shared_check.complete[0].checked_characters, 128);
    assert_eq!(ten_face_check.complete[0].checked_characters, 128);

    let mut group = c.benchmark_group("coverage");
    group.throughput(Throughput::Elements(COVERAGE_REFERENCES as u64));
    group.bench_function("shared_face_100_references", |b| {
        b.iter(|| black_box(&shared).check_coverage())
    });
    group.bench_function("ten_faces_100_references", |b| {
        b.iter(|| black_box(&ten_faces).check_coverage())
    });
    group.finish();
}

criterion_group!(
    benches, extraction, indexing, resolution, scanning, coverage
);
criterion_main!(benches);
