use crate::{
    normalize_name,
    syntax::{self, Piece, Record, diagnostic},
};
use serde::Serialize;
use std::{collections::BTreeMap, fs, io, path::Path};

/// A font request applied to non-whitespace dialogue text.
///
/// Extraction groups requests by normalized name, weight, and italic state.
/// The same font name can therefore appear in several references.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct FontReference {
    /// First encountered spelling, trimmed but retaining a possible `@` prefix.
    pub name: String,
    /// Requested weight: 400 for normal, 700 for bold, or an explicit ASS weight.
    pub weight: u16,
    /// Whether the subtitle requests italic text; not the selected face's property.
    pub italic: bool,
    /// Whether any visible text used this request through a vertical `@` name.
    ///
    /// Vertical text relies on the `vert`/`vrt2` OpenType features, so a font
    /// subsetter should retain them for such references.
    pub vertical: bool,
    /// One-based source line numbers, sorted and unique.
    pub lines: Vec<usize>,
    /// Distinct visible characters assigned to this request, sorted by code point.
    ///
    /// ASS line breaks, whitespace, and control characters are omitted; see
    /// [`Self::whitespace`].
    pub characters: Vec<CharacterUsage>,
    /// Distinct whitespace characters rendered with this request, sorted by code point.
    ///
    /// `\h` is recorded as U+00A0 and `\n` conservatively as U+0020 (a soft
    /// break renders as a space unless wrap style 2 is active); `\N` and control
    /// characters are omitted. Whitespace-only runs never create a reference, but
    /// their whitespace is attached to a reference with the same name, weight,
    /// and italic state when one exists. A subsetter should keep these glyphs so
    /// that the renderer does not fall back to another font for spacing.
    /// Coverage checks ignore this field unless
    /// [`CoverageOptions::include_whitespace`](crate::CoverageOptions::include_whitespace) is set.
    pub whitespace: Vec<CharacterUsage>,
}

/// One character assigned to a font request and the subtitle lines that use it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CharacterUsage {
    /// Unicode scalar value found in dialogue text.
    pub character: char,
    /// One-based source line numbers containing this character, sorted and unique.
    pub lines: Vec<usize>,
}

impl CharacterUsage {
    /// Format the character as an uppercase Unicode code point such as `U+0041`.
    pub fn codepoint(&self) -> String {
        format!("U+{:04X}", self.character as u32)
    }
}

struct Style<'a> {
    font: &'a str,
    weight: u16,
    italic: bool,
}

/// A recoverable subtitle parsing issue, including any fallback taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// One-based source line number.
    pub line: usize,
    /// Human-readable explanation; wording is not a stable machine-readable code.
    pub message: String,
}

/// Extracted requests and recoverable parsing diagnostics.
///
/// Empty diagnostics do not validate the entire ASS format or prove visibility:
/// timing, clipping, and transparency are not evaluated.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SubtitleFonts {
    /// Sorted by normalized name, then weight, then italic state.
    pub references: Vec<FontReference>,
    /// Issues sorted by source line; partial references may still be useful.
    pub diagnostics: Vec<Diagnostic>,
}

/// Read and parse a subtitle file.
///
/// Without the `encoding` feature, accepts UTF-8 (optional BOM) and BOM-marked
/// UTF-16 LE/BE; legacy encodings must be decoded by the caller and passed to
/// [`extract_fonts`]. With the `encoding` feature, the bytes are decoded by
/// `decode_subtitle`, which also detects legacy encodings such as GBK, Big5, and
/// Shift_JIS. The entire file is loaded into memory. The filename extension is
/// not checked.
///
/// # Errors
///
/// Returns filesystem errors unchanged, or [`io::ErrorKind::InvalidData`] for
/// bytes that cannot be decoded, including an odd UTF-16 byte count. Recoverable
/// subtitle syntax problems are returned in [`SubtitleFonts::diagnostics`], not
/// as I/O errors.
pub fn read_subtitle(path: impl AsRef<Path>) -> io::Result<SubtitleFonts> {
    let bytes = fs::read(path)?;
    let text = decode(bytes)?;
    Ok(extract_fonts(&text))
}

#[cfg(feature = "encoding")]
fn decode(bytes: Vec<u8>) -> io::Result<String> {
    crate::decode_subtitle(&bytes)
        .map(|decoded| decoded.text)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(not(feature = "encoding"))]
fn decode(bytes: Vec<u8>) -> io::Result<String> {
    let invalid = |e: String| io::Error::new(io::ErrorKind::InvalidData, e);
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return Err(invalid("odd UTF-16 byte length".into()));
        }
        let little = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|e| invalid(e.to_string()))
    } else {
        String::from_utf8(bytes).map_err(|e| invalid(e.to_string()))
    }
}

/// Extract requests from decoded ASS v4+ or SSA v4 text.
///
/// Uses `Format` column order (standard fields when absent); event `Text` must be
/// last and may contain commas. Styles may be defined after their dialogues.
/// Tracks style fonts, `\fn`, `\b`, `\i`, `\r`, named resets, and `\p` drawing mode.
/// Unused styles, comment events, override-block contents, drawing data, and
/// whitespace-only runs do not contribute references. Each reference also records
/// its distinct visible and whitespace characters and their source lines. Tags inside
/// parenthesized arguments are not applied as top-level overrides.
///
/// # Recovery
///
/// Malformed records are skipped with diagnostics. Unknown dialogue styles try
/// `Default`; unknown named resets try the original dialogue style. Duplicate
/// style definitions use the last definition. An unclosed override block is
/// diagnosed and treated as literal text. Unrecognized sections are ignored:
/// an unrelated input string may simply produce an empty result.
///
/// ```
/// let fonts = ass_fonts::extract_fonts(r"[V4+ Styles]
/// Format: Name, Fontname
/// Style: Default, Example
/// [Events]
/// Format: Style, Text
/// Dialogue: Default,Normal{\b1}Bold");
/// assert!(fonts.diagnostics.is_empty());
/// assert_eq!(fonts.references.len(), 2);
/// assert_eq!(fonts.references[0].weight, 400);
/// assert_eq!(fonts.references[1].weight, 700);
/// assert_eq!(fonts.references[1].lines, vec![6]);
/// ```
pub fn extract_fonts(text: &str) -> SubtitleFonts {
    let mut out = SubtitleFonts::default();
    let records = syntax::records(text, false, &mut out.diagnostics);
    let mut styles = BTreeMap::<&str, Style>::new();
    for record in &records {
        if let Record::Style {
            line,
            name,
            font,
            weight,
            italic,
        } = *record
        {
            let style = Style {
                font,
                weight,
                italic,
            };
            if styles.insert(name, style).is_some() {
                diagnostic(
                    &mut out.diagnostics,
                    line,
                    format!("duplicate style {name:?}; last definition wins"),
                );
            }
        }
    }
    let mut references = References::default();
    for record in &records {
        let Record::Event {
            line, style, text, ..
        } = *record
        else {
            continue;
        };
        let base = styles.get(style).or_else(|| styles.get("Default"));
        if !styles.contains_key(style) {
            diagnostic(
                &mut out.diagnostics,
                line,
                format!("unknown style {style:?}; using Default if available"),
            );
        }
        let mut state = State::new(base);
        syntax::pieces(text, |piece| match piece {
            Piece::Text(text) => references.collect(text, &state, line),
            Piece::Tag(tag) => apply_tag(tag, base, &styles, &mut state, line, &mut out),
            Piece::Unclosed => diagnostic(
                &mut out.diagnostics,
                line,
                "unclosed override block; treated as literal text",
            ),
        });
    }
    out.references = references.finish();
    out.diagnostics.sort_by_key(|d| d.line);
    out
}

struct State<'a> {
    font: Option<&'a str>,
    style: Option<&'a Style<'a>>,
    weight: u16,
    italic: bool,
    drawing: bool,
}

impl<'a> State<'a> {
    fn new(style: Option<&'a Style<'a>>) -> Self {
        Self {
            font: style.map(|s| s.font),
            style,
            weight: style.map_or(400, |s| s.weight),
            italic: style.is_some_and(|s| s.italic),
            drawing: false,
        }
    }
}

type Key = (String, u16, bool);

#[derive(Default)]
struct References {
    visible: BTreeMap<Key, FontReference>,
    // Collected separately: whitespace alone does not create a reference.
    whitespace: BTreeMap<Key, Vec<CharacterUsage>>,
}

impl References {
    fn collect(&mut self, text: &str, state: &State<'_>, line: usize) {
        if state.drawing {
            return;
        }
        let Some(font) = state.font.filter(|s| !normalize_name(s).is_empty()) else {
            return;
        };
        let (characters, whitespace) = rendered_characters(text);
        let key = (normalize_name(font), state.weight, state.italic);
        if !whitespace.is_empty() {
            let usages = self.whitespace.entry(key.clone()).or_default();
            for character in whitespace {
                record(usages, character, line);
            }
        }
        if characters.is_empty() {
            return;
        }
        let entry = self.visible.entry(key).or_insert_with(|| FontReference {
            name: font.trim().into(),
            weight: state.weight,
            italic: state.italic,
            ..FontReference::default()
        });
        entry.vertical |= font.trim_start().starts_with('@');
        if entry.lines.last() != Some(&line) {
            entry.lines.push(line);
        }
        for character in characters {
            record(&mut entry.characters, character, line);
        }
    }

    fn finish(mut self) -> Vec<FontReference> {
        for (key, reference) in &mut self.visible {
            if let Some(whitespace) = self.whitespace.remove(key) {
                reference.whitespace = whitespace;
            }
        }
        self.visible.into_values().collect()
    }
}

fn record(usages: &mut Vec<CharacterUsage>, character: char, line: usize) {
    match usages.binary_search_by_key(&character, |usage| usage.character) {
        Ok(index) => {
            let lines = &mut usages[index].lines;
            if lines.last() != Some(&line) {
                lines.push(line);
            }
        }
        Err(index) => usages.insert(
            index,
            CharacterUsage {
                character,
                lines: vec![line],
            },
        ),
    }
}

/// Distinct visible characters and distinct whitespace characters of a text run.
fn rendered_characters(text: &str) -> (Vec<char>, Vec<char>) {
    fn insert(set: &mut Vec<char>, character: char) {
        if let Err(index) = set.binary_search(&character) {
            set.insert(index, character);
        }
    }
    let mut characters = Vec::new();
    let mut whitespace = Vec::new();
    let mut input = text.chars().peekable();
    while let Some(character) = input.next() {
        if character == '\\' {
            let escaped = match input.peek() {
                Some('N') => Some(None),
                // A soft break renders as a space unless wrap style 2 is active.
                Some('n') => Some(Some(' ')),
                Some('h') => Some(Some('\u{a0}')),
                _ => None,
            };
            if let Some(space) = escaped {
                input.next();
                if let Some(space) = space {
                    insert(&mut whitespace, space);
                }
                continue;
            }
        }
        if character.is_control() {
            continue;
        }
        if character.is_whitespace() {
            insert(&mut whitespace, character);
        } else {
            insert(&mut characters, character);
        }
    }
    (characters, whitespace)
}

fn apply_tag<'a>(
    tag: &'a str,
    base: Option<&'a Style<'a>>,
    styles: &'a BTreeMap<&'a str, Style<'a>>,
    state: &mut State<'a>,
    line: usize,
    out: &mut SubtitleFonts,
) {
    // Font name and reset are not animatable ASS tags, so tags nested in
    // parenthesized arguments never reach this function.
    if let Some(name) = syntax::font_tag(tag) {
        state.font = if name.is_empty() {
            state.style.map(|s| s.font)
        } else {
            Some(name)
        };
    } else if let Some(name) = tag.strip_prefix('r') {
        let name = name.trim();
        let style = if name.is_empty() {
            base
        } else {
            let font = styles.get(name);
            if font.is_none() {
                diagnostic(
                    &mut out.diagnostics,
                    line,
                    format!("unknown reset style {name:?}; using dialogue style"),
                );
            }
            font.or(base)
        };
        *state = State::new(style);
    } else if let Some(value) = property(tag, 'b') {
        if value.is_empty() {
            state.weight = state.style.map_or(400, |s| s.weight);
        } else if let Some(weight) = syntax::weight(value) {
            state.weight = weight;
        } else {
            diagnostic(
                &mut out.diagnostics,
                line,
                format!("invalid bold override {tag:?}; ignored"),
            );
        }
    } else if let Some(value) = property(tag, 'i') {
        if value.is_empty() {
            state.italic = state.style.is_some_and(|s| s.italic);
        } else if let Some(italic) = syntax::italic(value) {
            state.italic = italic;
        } else {
            diagnostic(
                &mut out.diagnostics,
                line,
                format!("invalid italic override {tag:?}; ignored"),
            );
        }
    } else if let Some(value) = tag
        .strip_prefix('p')
        .and_then(|v| v.trim().parse::<i32>().ok())
    {
        state.drawing = value > 0;
    }
}

// Do not mistake bord/be/blur/iclip for the single-letter b/i tags.
fn property(tag: &str, prefix: char) -> Option<&str> {
    let value = tag.strip_prefix(prefix)?.trim();
    if value.starts_with(|c: char| c.is_ascii_alphabetic()) {
        None
    } else {
        Some(value)
    }
}
