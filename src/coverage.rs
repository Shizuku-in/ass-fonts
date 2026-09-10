use crate::{CharacterUsage, FontFace, FontReference, MissingReason, Resolution, ResolveReport};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

/// Nominal cmap coverage for one selected font face.
///
/// Ambiguous font resolutions produce one check per candidate. A complete check
/// only means that every checked Unicode scalar has a nominal cmap mapping; it
/// does not validate shaping, positioning, color layers, or rendering quality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageCheck {
    /// Original font request and its extracted character usage.
    pub reference: FontReference,
    /// Selected face whose cmap was checked.
    pub candidate: FontFace,
    /// Number of distinct characters checked.
    pub checked_characters: usize,
    /// Characters without a nominal cmap mapping, sorted by code point.
    pub missing_characters: Vec<CharacterUsage>,
}

/// Why nominal glyph coverage could not be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageUnavailableReason {
    /// Font matching did not select a candidate.
    NoCandidate,
    /// The selected font file could not be read after it was scanned or supplied.
    FontReadError,
    /// The requested face could not be parsed from the selected file.
    FaceParseError,
}

/// A font request or candidate for which coverage is unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UncheckableCoverage {
    /// Original font request and its extracted character usage.
    pub reference: FontReference,
    /// Selected face, or `None` when font matching found no candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<FontFace>,
    /// Matching reason when `candidate` is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_reason: Option<MissingReason>,
    /// Stable category for the unavailable check.
    pub reason: CoverageUnavailableReason,
    /// Human-readable filesystem or parser detail; wording is not stable.
    pub message: String,
}

/// Candidate-level nominal cmap results.
///
/// Each resolved candidate occurs exactly once in `complete`, `incomplete`, or
/// `uncheckable`. A missing font resolution contributes one candidate-less entry
/// to `uncheckable`. Entries preserve resolution and candidate order within each
/// output bucket.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageReport {
    /// Candidates mapping every checked character.
    pub complete: Vec<CoverageCheck>,
    /// Candidates missing at least one checked character.
    pub incomplete: Vec<CoverageCheck>,
    /// Requests or candidates that could not be checked.
    pub uncheckable: Vec<UncheckableCoverage>,
}

/// Compact candidate and missing-character counts for a [`CoverageReport`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CoverageSummary {
    /// Selected candidates mapping every checked character.
    pub complete_candidates: usize,
    /// Selected candidates missing at least one checked character.
    pub incomplete_candidates: usize,
    /// Selected candidates whose files or faces could not be checked.
    pub uncheckable_candidates: usize,
    /// Font references for which matching selected no candidate.
    pub unresolved_references: usize,
    /// Sum of missing characters across incomplete candidates.
    pub missing_mappings: usize,
    /// Distinct missing Unicode scalar values across incomplete candidates.
    pub unique_missing_characters: usize,
}

impl CoverageReport {
    /// Return compact counts without discarding the detailed report.
    pub fn summary(&self) -> CoverageSummary {
        let missing = self
            .incomplete
            .iter()
            .flat_map(|check| &check.missing_characters);
        let mut unique = BTreeSet::new();
        let mut missing_mappings = 0;
        for usage in missing {
            missing_mappings += 1;
            unique.insert(usage.character);
        }
        CoverageSummary {
            complete_candidates: self.complete.len(),
            incomplete_candidates: self.incomplete.len(),
            uncheckable_candidates: self
                .uncheckable
                .iter()
                .filter(|entry| entry.candidate.is_some())
                .count(),
            unresolved_references: self
                .uncheckable
                .iter()
                .filter(|entry| entry.candidate.is_none())
                .count(),
            missing_mappings,
            unique_missing_characters: unique.len(),
        }
    }

    /// Whether every selected candidate has complete nominal coverage.
    ///
    /// Returns `false` for missing font references and unreadable or invalid
    /// candidates. An empty report is complete.
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_empty() && self.uncheckable.is_empty()
    }

    /// Aggregate missing characters across all candidates.
    ///
    /// Characters and their source lines are sorted and deduplicated. Candidate
    /// identity is intentionally discarded; use [`Self::incomplete`] when it matters.
    pub fn missing_characters(&self) -> Vec<CharacterUsage> {
        let mut characters = BTreeMap::<char, BTreeSet<usize>>::new();
        for usage in self
            .incomplete
            .iter()
            .flat_map(|check| &check.missing_characters)
        {
            characters
                .entry(usage.character)
                .or_default()
                .extend(&usage.lines);
        }
        characters
            .into_iter()
            .map(|(character, lines)| CharacterUsage {
                character,
                lines: lines.into_iter().collect(),
            })
            .collect()
    }
}

/// Reopen selected font files and check extracted characters against each face's cmap.
///
/// Font files are cached by path for the duration of this call. Variation selectors,
/// joiners, and directional formatting controls are skipped because they do not
/// require an independent nominal glyph. Whitespace and control characters have
/// already been omitted by subtitle extraction.
///
/// This does not apply font fallback or shape text. A successful result therefore
/// establishes nominal character mapping only, not renderer-identical output.
pub fn check_coverage(resolutions: &ResolveReport) -> CoverageReport {
    let mut report = CoverageReport::default();
    let mut files = BTreeMap::<PathBuf, Result<Vec<u8>, String>>::new();

    for resolution in resolutions.resolved.iter().chain(&resolutions.ambiguous) {
        for candidate in &resolution.candidates {
            let data = files
                .entry(candidate.path.clone())
                .or_insert_with(|| fs::read(&candidate.path).map_err(|error| error.to_string()));
            let data = match data {
                Ok(data) => data,
                Err(message) => {
                    report.uncheckable.push(uncheckable(
                        resolution,
                        Some(candidate.clone()),
                        CoverageUnavailableReason::FontReadError,
                        message.clone(),
                    ));
                    continue;
                }
            };
            let face = match ttf_parser::Face::parse(data, candidate.face_index) {
                Ok(face) => face,
                Err(error) => {
                    report.uncheckable.push(uncheckable(
                        resolution,
                        Some(candidate.clone()),
                        CoverageUnavailableReason::FaceParseError,
                        error.to_string(),
                    ));
                    continue;
                }
            };
            let checked = resolution
                .reference
                .characters
                .iter()
                .filter(|usage| requires_nominal_glyph(usage.character));
            let mut check = CoverageCheck {
                reference: resolution.reference.clone(),
                candidate: candidate.clone(),
                checked_characters: 0,
                missing_characters: Vec::new(),
            };
            for usage in checked {
                check.checked_characters += 1;
                if face
                    .glyph_index(usage.character)
                    .is_none_or(|glyph| glyph.0 == 0)
                {
                    check.missing_characters.push(usage.clone());
                }
            }
            if check.missing_characters.is_empty() {
                report.complete.push(check);
            } else {
                report.incomplete.push(check);
            }
        }
    }

    for resolution in &resolutions.missing {
        report.uncheckable.push(uncheckable(
            resolution,
            None,
            CoverageUnavailableReason::NoCandidate,
            "font resolution has no candidate".into(),
        ));
    }
    report
}

fn uncheckable(
    resolution: &Resolution,
    candidate: Option<FontFace>,
    reason: CoverageUnavailableReason,
    message: String,
) -> UncheckableCoverage {
    UncheckableCoverage {
        reference: resolution.reference.clone(),
        candidate,
        missing_reason: resolution.missing_reason,
        reason,
        message,
    }
}

fn requires_nominal_glyph(character: char) -> bool {
    !matches!(
        character,
        // Soft hyphen, combining grapheme joiner, Arabic letter mark.
        '\u{00ad}' | '\u{034f}' | '\u{061c}' |
        // Mongolian free variation selectors and format control.
        '\u{180b}'..='\u{180f}' |
        // Zero-width spaces/joiners and bidirectional controls.
        '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' |
        // BMP variation selectors and byte-order mark.
        '\u{fe00}'..='\u{fe0f}' | '\u{feff}' |
        // Interlinear annotation and shorthand format controls.
        '\u{fff9}'..='\u{fffb}' |
        // Tags and supplementary variation selectors.
        '\u{e0000}'..='\u{e007f}' | '\u{e0100}'..='\u{e01ef}'
    )
}
