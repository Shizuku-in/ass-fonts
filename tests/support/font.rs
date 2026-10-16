fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

pub fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Font metadata for [`sfnt`]; `base` offsets table records inside a collection.
pub struct Spec<'a> {
    pub names: &'a [(u16, &'a str)],
    pub characters: &'a [char],
    pub weight: u16,
    pub italic: bool,
    pub otf: bool,
    pub base: usize,
}

// A minimal synthetic font with a format 12 cmap. It is sufficient for metadata
// and cmap tests without relying on installed or redistributable fonts.
#[allow(dead_code)] // Not every test crate including this module uses it.
pub fn font(name: &str, characters: &[char]) -> Vec<u8> {
    sfnt(Spec {
        names: &[(1, name)],
        characters,
        weight: 400,
        italic: false,
        otf: false,
        base: 0,
    })
}

pub fn sfnt(spec: Spec) -> Vec<u8> {
    sfnt_with_records(spec, &[])
}

/// Like [`sfnt`], appending raw `(platform, encoding, name ID, bytes)` records
/// after the Windows Unicode records generated from `spec.names`.
#[allow(dead_code)] // Only some test crates build legacy name records.
pub fn sfnt_with_records(spec: Spec, raw: &[(u16, u16, u16, &[u8])]) -> Vec<u8> {
    let mut characters = spec.characters.to_vec();
    characters.sort_unstable();
    characters.dedup();

    let mut records: Vec<(u16, u16, u16, Vec<u8>)> = spec
        .names
        .iter()
        .map(|(id, text)| {
            // Windows Unicode BMP
            (
                3,
                1,
                *id,
                text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            )
        })
        .collect();
    records.extend(
        raw.iter()
            .map(|&(p, e, id, bytes)| (p, e, id, bytes.to_vec())),
    );
    let storage = 6 + 12 * records.len();
    let mut naming = vec![0; storage];
    put16(&mut naming, 2, records.len() as u16);
    put16(&mut naming, 4, storage as u16);
    for (index, (platform, encoding, id, bytes)) in records.into_iter().enumerate() {
        let record = 6 + index * 12;
        let offset = naming.len() - storage;
        put16(&mut naming, record, platform);
        put16(&mut naming, record + 2, encoding);
        put16(
            &mut naming,
            record + 4,
            if platform == 3 { 0x409 } else { 0 },
        );
        put16(&mut naming, record + 6, id);
        put16(&mut naming, record + 8, bytes.len() as u16);
        put16(&mut naming, record + 10, offset as u16);
        naming.extend(bytes);
    }

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
    put16(&mut os2, 4, spec.weight);
    put16(&mut os2, 6, 5);
    put16(&mut os2, 62, u16::from(spec.italic));

    let tables = [
        (*b"OS/2", os2),
        (*b"cmap", cmap),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"name", naming),
    ];
    let mut bytes = vec![0; 12 + 16 * tables.len()];
    let version = if spec.otf {
        u32::from_be_bytes(*b"OTTO")
    } else {
        0x10000
    };
    put32(&mut bytes, 0, version);
    put16(&mut bytes, 4, tables.len() as u16);
    for (index, (tag, table)) in tables.into_iter().enumerate() {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let offset = bytes.len();
        let record = 12 + index * 16;
        bytes[record..record + 4].copy_from_slice(&tag);
        put32(&mut bytes, record + 8, (spec.base + offset) as u32);
        put32(&mut bytes, record + 12, table.len() as u32);
        bytes.extend(table);
    }
    bytes
}
