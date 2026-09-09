//! Extract fonts used by ASS/SSA dialogue and resolve them using font name tables.
//!
//! Family matches use weight and italic attributes; glyph coverage and
//! renderer-specific fallback are outside this library's scope.
mod fonts;
mod subtitle;

pub use fonts::{
    FontFace, FontIndex, FontName, MatchEvidence, MissingReason, NameKind, Resolution,
    ResolveReport, ScanIssue, ScanReport,
};
pub use subtitle::{Diagnostic, FontReference, SubtitleFonts, extract_fonts, read_subtitle};

/// Normalize a font name for matching. ASS's vertical-font prefix is ignored.
pub fn normalize_name(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.trim()
        .trim_start_matches('@')
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
