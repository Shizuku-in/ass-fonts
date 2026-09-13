mod index;
mod resolve;
mod scan;

use crate::{FontReference, normalize_name};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

/// Owned metadata for one face in a font file or collection.
///
/// Identity is `(path, face_index)`, not the name or file contents. No font bytes
/// are retained. Scanned names include Unicode records and Macintosh Roman ASCII;
/// unsupported legacy encodings are skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FontFace {
    /// Canonical file path when produced by [`ScanReport::scan`].
    pub path: PathBuf,
    /// Zero-based collection index; standalone TTF/OTF files use zero.
    pub face_index: u32,
    /// All decoded matching aliases, sorted and deduplicated by the scanner.
    pub names: Vec<String>,
    /// Family aliases (name IDs 1/16/21), used for weight/italic filtering.
    pub family_names: Vec<String>,
    /// Typed internal names for reporting match provenance. Untyped `names`
    /// supplied by callers are reported as `internal_name`.
    pub name_records: Vec<FontName>,
    /// Native numeric weight from the parser (normally 1–1000); 400 if OS/2 is absent.
    /// This value is not changed by matching or synthesis.
    pub weight: u16,
    /// Includes fonts marked as oblique.
    pub italic: bool,
}

/// Semantic role of an internal alias; related OpenType IDs share a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    /// Legacy, typographic, or WWS family (IDs 1, 16, 21).
    Family,
    /// Full or compatible full name (IDs 4, 18).
    FullName,
    /// PostScript name (ID 6).
    PostScriptName,
    /// Caller-supplied alias without known name-table provenance.
    InternalName,
}

/// An original decoded alias and its classification.
///
/// For caller-built records, keep `kind` consistent with `name_id`, or use
/// `name_id: None` when the original ID is unknown. The index does not validate
/// contradictory caller-supplied metadata.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FontName {
    /// Original decoded spelling, before matching normalization.
    pub name: String,
    /// Name role used for selection and evidence reporting.
    pub kind: NameKind,
    /// Original OpenType name ID; absent for caller-supplied aliases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_id: Option<u16>,
}

/// A filesystem or face parsing problem encountered while scanning.
#[derive(Debug, Clone, Serialize)]
pub struct ScanIssue {
    /// Affected file or directory; canonicalization may not have succeeded.
    pub path: PathBuf,
    /// Affected face, or `None` for a filesystem or collection-level failure.
    pub face_index: Option<u32>,
    /// Human-readable error, not a stable machine-readable code.
    pub message: String,
}

/// Successfully parsed faces and non-fatal scan issues.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanReport {
    /// Faces sorted by canonical path and collection index.
    pub faces: Vec<FontFace>,
    /// Problems encountered; inspect even when matching reports no missing fonts.
    pub issues: Vec<ScanIssue>,
}

/// Why no candidate could be selected under the requested options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    /// No normalized internal alias matched the requested name.
    NameNotFound,
    /// The family exists, but no face satisfies the current weight/italic policy.
    StyleNotFound,
}

/// All name records that matched a candidate under the selected matching rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchEvidence {
    /// Same path as the corresponding candidate.
    pub path: PathBuf,
    /// Same collection index as the corresponding candidate.
    pub face_index: u32,
    /// Matching name records that support the selected method, in sorted order.
    pub matched_names: Vec<FontName>,
    /// True only when bold synthesis was permitted and the 400→700 rule applies.
    /// False does not guarantee visual fidelity or that a renderer will not synthesize.
    /// No font is generated and actual rendering is not checked.
    pub synthetic_bold: bool,
    /// True when italic synthesis is permitted and an italic request selects an
    /// upright face. No glyphs are transformed by this library.
    pub synthetic_italic: bool,
    /// How this candidate was selected, independently of the synthesis flag.
    pub selection_method: SelectionMethod,
}

/// Selection path used for a candidate; serialized using snake_case names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMethod {
    /// Exact normalized PostScript alias, ahead of family and full-name matches.
    PostScriptName,
    /// Full name sufficiently distinct from generic family aliases.
    FullName,
    /// Legacy family alias belonging to a broader typographic family and one style.
    LegacyFamilyName,
    /// Family candidate with exact requested weight and italic state.
    FamilyExact,
    /// Family candidate at minimum weight distance in the selected slant tier.
    FamilyNearest,
    /// A permitted bold or italic fallback selected this face without weight approximation.
    FamilySynthesis,
    /// An alias without a more specific selection classification.
    InternalName,
}

/// Weight selection policy for generic families, not specific face names.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WeightMatching {
    #[default]
    /// Require numeric equality. Separate synthesis permission can still allow fallback.
    Exact,
    /// Minimum absolute weight difference among faces in the selected family/slant tier.
    /// All equally close faces are retained; there is no distance cutoff.
    /// Exact matches are preferred and reported as [`SelectionMethod::FamilyExact`].
    /// This absolute-distance policy does not emulate CSS or a specific renderer.
    Nearest,
}

/// Permitted renderer-side synthesis, disabled by default.
///
/// Permission records a dependency on synthesis; this library does not alter glyphs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SynthesisPolicy {
    /// Permit weight 400 to satisfy weight 700. Can combine with italic synthesis.
    /// No other synthetic weight conversions are implemented.
    pub bold: bool,
    /// Permit an upright face for an italic request if no native-italic candidate
    /// satisfies the weight policy. Never substitutes italic for upright text.
    pub italic: bool,
}

/// Independent controls for family weight selection and style synthesis.
///
/// Defaults are [`WeightMatching::Exact`] and no synthesis. The options do not
/// change alias normalization or specific-name selection.
///
/// ```
/// use ass_fonts::{ResolveOptions, SynthesisPolicy, WeightMatching};
/// let options = ResolveOptions {
///     weight_matching: WeightMatching::Nearest,
///     synthesis: SynthesisPolicy { bold: true, italic: true },
/// };
/// assert_eq!(options.weight_matching, WeightMatching::Nearest);
/// assert!(!ResolveOptions::default().synthesis.bold);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResolveOptions {
    /// Exact or minimum-distance selection within a matching family.
    pub weight_matching: WeightMatching,
    /// Synthesis permission evaluated independently of weight selection.
    pub synthesis: SynthesisPolicy,
}

impl From<ResolveMode> for ResolveOptions {
    fn from(mode: ResolveMode) -> Self {
        Self {
            synthesis: SynthesisPolicy {
                bold: mode == ResolveMode::AllowStyleSynthesis,
                italic: mode == ResolveMode::AllowStyleSynthesis,
            },
            ..Self::default()
        }
    }
}

/// Compatibility presets for [`FontIndex::resolve_with_mode`].
///
/// Both presets use exact weight matching. Use [`ResolveOptions`] to enable nearest
/// weight selection; converting a preset with `ResolveOptions::from` preserves this.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveMode {
    #[default]
    /// Exact family matching with synthesis disabled.
    Strict,
    /// Allow both renderer-side bold (400→700) and italic (upright→italic) synthesis.
    /// Exact family matches always take precedence. This is a collection policy,
    /// not an emulation of a particular renderer's font selection algorithm.
    AllowStyleSynthesis,
}

/// One input reference and its selection result.
///
/// Produced entries in [`ResolveReport`] have zero, one, or multiple candidates
/// according to their bucket. Optional missing information is omitted from JSON
/// when absent; empty `candidates` and `matches` arrays are retained.
#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    /// Original request, including its source lines and requested attributes.
    pub reference: FontReference,
    /// Selected native faces, sorted by path and face index.
    pub candidates: Vec<FontFace>,
    /// Present only for missing entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_reason: Option<MissingReason>,
    /// All scanned faces of the matching family, only for `style_not_found`.
    /// These are informational, not successful candidates or fallbacks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available_variants: Vec<FontFace>,
    /// One entry per candidate, in the same order. Empty for missing entries.
    pub matches: Vec<MatchEvidence>,
}

/// Requests partitioned by candidate count, preserving input order in each bucket.
///
/// Matching does not deduplicate input references. A resolved entry identifies a
/// dependency, not glyph coverage or exact rendering. Inspect scan and subtitle
/// diagnostics separately; they are not embedded in this report.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ResolveReport {
    /// Exactly one candidate per entry.
    pub resolved: Vec<Resolution>,
    /// No candidates; each entry contains a [`MissingReason`].
    pub missing: Vec<Resolution>,
    /// Two or more candidates, including equally close weights and duplicate files.
    pub ambiguous: Vec<Resolution>,
}

impl ResolveReport {
    /// Check nominal glyph coverage for every selected candidate.
    ///
    /// This reopens font files and is equivalent to [`crate::check_coverage`].
    /// See that function for cmap and rendering limitations.
    pub fn check_coverage(&self) -> crate::CoverageReport {
        crate::check_coverage(self)
    }
}

/// Reusable index of all supported internal names, with deterministic ordering.
#[derive(Debug, Default)]
pub struct FontIndex {
    faces: Vec<FontFace>,
    names: BTreeMap<String, BTreeSet<usize>>,
    families: BTreeMap<String, BTreeSet<usize>>,
    full_names: BTreeMap<String, BTreeSet<usize>>,
    postscript_names: BTreeMap<String, BTreeSet<usize>>,
    generic_families: BTreeSet<String>,
}
