fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

// A minimal synthetic TrueType font with a format 12 cmap. It is sufficient for
// metadata and cmap tests without relying on installed or redistributable fonts.
pub fn font(name: &str, characters: &[char]) -> Vec<u8> {
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
