//! Extract fonts used by ASS/SSA dialogue and resolve them using font name tables.
//!
//! # Workflow
//!
//! 1. Read a subtitle with [`read_subtitle`], or parse decoded text with [`extract_fonts`].
//! 2. Scan explicit font files or directories with [`ScanReport::scan`].
//! 3. Build a reusable [`FontIndex`] and resolve references with [`ResolveOptions`].
//! 4. Optionally call [`ResolveReport::check_coverage`] for nominal cmap coverage.
//! 5. Inspect the reports together with subtitle and scanning diagnostics.
//!
//! [`rewrite_font_names`] replaces font names using the same parser, for example
//! to obfuscate names before muxing subset fonts.
//!
//! ```no_run
//! use ass_fonts::{FontIndex, ResolveOptions, ScanReport, SynthesisPolicy,
//!                 WeightMatching, read_subtitle};
//!
//! # fn main() -> std::io::Result<()> {
//! let subtitle = read_subtitle("movie.ass")?;
//! let scan = ScanReport::scan(["./fonts"]);
//! let index = FontIndex::new(scan.faces);
//! let report = index.resolve_with_options(&subtitle.references, ResolveOptions {
//!     weight_matching: WeightMatching::Nearest,
//!     synthesis: SynthesisPolicy { bold: true, italic: true },
//! });
//! println!("{} resolved, {} missing, {} ambiguous",
//!     report.resolved.len(), report.missing.len(), report.ambiguous.len());
//! // Successful matching does not imply that every input was parsed or scanned.
//! eprintln!("{:?}", subtitle.diagnostics);
//! eprintln!("{:?}", scan.issues);
//! # Ok(())
//! # }
//! ```
//!
//! # Matching and limits
//!
//! Specific font names locate faces; generic family names use weight and italic
//! attributes. Defaults require exact family attributes and disable synthesis.
//! Nearest-weight selection, bold synthesis, and italic synthesis are independent options.
//! See [`FontIndex::resolve_with_options`] for selection precedence.
//!
//! A resolved dependency identifies a font file, not a guarantee of visual fidelity.
//! Nominal cmap coverage can be checked separately; shaping, rendering,
//! cross-family fallback, variable-font instances, embedded font extraction, and
//! system font directory discovery are outside scope.
//! Reports implement [`serde::Serialize`]; enum values use snake_case in JSON.
//!
//! # Features
//!
//! - `legacy-names` (default): decode CJK legacy-encoded font name records.
//! - `encoding`: detect and decode legacy subtitle encodings with
//!   `decode_subtitle`; [`read_subtitle`] then uses it.
//! - `parallel`: scan font files in parallel with rayon.
mod coverage;
#[cfg(feature = "encoding")]
mod encoding;
mod fonts;
mod rewrite;
mod subtitle;
mod syntax;

pub use coverage::{
    CoverageCheck, CoverageOptions, CoverageReport, CoverageSummary, CoverageUnavailableReason,
    UncheckableCoverage, check_coverage, check_coverage_with,
};
#[cfg(feature = "encoding")]
pub use encoding::{DecodeError, DecodedSubtitle, decode_subtitle};
pub use fonts::{
    FontFace, FontIndex, FontName, MatchEvidence, MissingReason, NameKind, Resolution, ResolveMode,
    ResolveOptions, ResolveReport, SCAN_FORMAT_VERSION, ScanIssue, ScanProgress, ScanReport,
    SelectionMethod, SynthesisPolicy, WeightMatching, discover_font_files, scan_font_file,
};
pub use rewrite::{FontNameSite, Rewritten, SiteKind, rewrite_font_names};
pub use subtitle::{
    CharacterUsage, Diagnostic, FontReference, SubtitleFonts, extract_fonts, read_subtitle,
};

/// Normalize a font name for exact alias comparison.
///
/// Trims outer whitespace, removes leading ASS vertical-font `@` prefixes, applies
/// Unicode NFKC and lowercase conversion, then collapses whitespace to single
/// spaces. This is neither fuzzy matching nor complete Unicode case folding.
/// Punctuation and meaningful internal spaces are preserved.
///
/// ```
/// assert_eq!(ass_fonts::normalize_name("  @ＥＸＡＭＰＬＥ  Sans  "), "example sans");
/// assert_eq!(ass_fonts::normalize_name("@@"), "");
/// ```
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
