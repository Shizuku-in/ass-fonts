#![no_main]

use ass_fonts::{FontIndex, ResolveMode, extract_fonts};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let subtitle = extract_fonts(text);
        let index = FontIndex::default();
        let _ = index.resolve(&subtitle.references);
        let _ = index.resolve_with_mode(&subtitle.references, ResolveMode::AllowStyleSynthesis);
    }
});
