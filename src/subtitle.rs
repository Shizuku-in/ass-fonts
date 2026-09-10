use crate::normalize_name;
use serde::Serialize;
use std::{collections::BTreeMap, fs, io, path::Path};

/// A font request applied to non-whitespace dialogue text.
///
/// Extraction groups requests by normalized name, weight, and italic state.
/// The same font name can therefore appear in several references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FontReference {
    /// First encountered spelling, trimmed but retaining a possible `@` prefix.
    pub name: String,
    /// Requested weight: 400 for normal, 700 for bold, or an explicit ASS weight.
    pub weight: u16,
    /// Whether the subtitle requests italic text; not the selected face's property.
    pub italic: bool,
    /// One-based source line numbers, sorted and unique.
    pub lines: Vec<usize>,
    /// Distinct rendered characters assigned to this request, sorted by code point.
    ///
    /// ASS hard/soft line breaks, hard spaces, Unicode whitespace, and control
    /// characters are omitted because they do not require a visible glyph.
    pub characters: Vec<CharacterUsage>,
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

struct Style {
    font: String,
    weight: u16,
    italic: bool,
}

fn weight(value: &str) -> Option<u16> {
    match value.trim().parse::<i32>().ok()? {
        0 => Some(400),
        -1 | 1 => Some(700),
        n @ 100..=900 => Some(n as u16),
        _ => None,
    }
}

fn italic(value: &str) -> Option<bool> {
    match value.trim().parse::<i32>().ok()? {
        0 => Some(false),
        -1 | 1 => Some(true),
        _ => None,
    }
}

/// A recoverable subtitle parsing issue, including any fallback taken.
#[derive(Debug, Clone, Serialize)]
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

/// Read and parse a UTF-8 or BOM-marked UTF-16 LE/BE subtitle file.
///
/// UTF-8 accepts an optional BOM. Legacy encodings must be decoded by the caller
/// and passed to [`extract_fonts`]; this function does not guess the encoding.
/// The entire file is loaded into memory. The filename extension is not checked.
///
/// # Errors
///
/// Returns filesystem errors unchanged, or [`io::ErrorKind::InvalidData`] for
/// invalid UTF-8/UTF-16, including an odd UTF-16 byte count. Recoverable subtitle
/// syntax problems are returned in [`SubtitleFonts::diagnostics`], not as I/O errors.
pub fn read_subtitle(path: impl AsRef<Path>) -> io::Result<SubtitleFonts> {
    let bytes = fs::read(path)?;
    let invalid = |e: String| io::Error::new(io::ErrorKind::InvalidData, e);
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
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
        String::from_utf16(&units).map_err(|e| invalid(e.to_string()))?
    } else {
        String::from_utf8(bytes).map_err(|e| invalid(e.to_string()))?
    };
    Ok(extract_fonts(&text))
}

fn columns(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect()
}

fn field<'a>(format: &[String], values: &[&'a str], name: &str) -> Option<&'a str> {
    format
        .iter()
        .position(|s| s == name)
        .and_then(|i| values.get(i).copied())
}

fn diagnostic(out: &mut SubtitleFonts, line: usize, message: impl Into<String>) {
    out.diagnostics.push(Diagnostic {
        line,
        message: message.into(),
    });
}

/// Extract requests from decoded ASS v4+ or SSA v4 text.
///
/// Uses `Format` column order (standard fields when absent); event `Text` must be
/// last and may contain commas. Styles may be defined after their dialogues.
/// Tracks style fonts, `\fn`, `\b`, `\i`, `\r`, named resets, and `\p` drawing mode.
/// Unused styles, comment events, override-block contents, drawing data, and
/// whitespace-only runs do not contribute references. Each reference also records
/// its distinct rendered characters and their source lines. Tags inside
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
    let mut styles = BTreeMap::<String, Style>::new();
    let mut events = Vec::new();
    let mut section = String::new();
    let mut style_format = Vec::new();
    let mut event_format =
        columns("Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text");
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = index + 1;
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with(';') {
            continue;
        }
        if raw.starts_with('[') && raw.ends_with(']') {
            section = raw.to_ascii_lowercase();
            if section == "[v4+ styles]" {
                style_format = columns(
                    "Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding",
                );
            } else if section == "[v4 styles]" {
                style_format = columns(
                    "Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,TertiaryColour,BackColour,Bold,Italic,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,AlphaLevel,Encoding",
                );
            }
            continue;
        }
        let Some((kind, value)) = raw.split_once(':') else {
            continue;
        };
        let kind = kind.trim().to_ascii_lowercase();
        let value = value.trim();
        if section == "[v4 styles]" || section == "[v4+ styles]" {
            if kind == "format" {
                style_format = columns(value);
            }
            if kind == "style" {
                let values: Vec<_> = value.split(',').map(str::trim).collect();
                if values.len() != style_format.len() {
                    diagnostic(&mut out, line, "style field count differs from Format");
                    continue;
                }
                match (
                    field(&style_format, &values, "name"),
                    field(&style_format, &values, "fontname"),
                ) {
                    (Some(name), Some(font))
                        if !name.is_empty() && !normalize_name(font).is_empty() =>
                    {
                        let bold = field(&style_format, &values, "bold").unwrap_or("0");
                        let slant = field(&style_format, &values, "italic").unwrap_or("0");
                        if weight(bold).is_none() || italic(slant).is_none() {
                            diagnostic(
                                &mut out,
                                line,
                                "invalid Bold/Italic; invalid properties use normal defaults",
                            );
                        }
                        let style = Style {
                            font: font.into(),
                            weight: weight(bold).unwrap_or(400),
                            italic: italic(slant).unwrap_or(false),
                        };
                        if styles.insert(name.into(), style).is_some() {
                            diagnostic(
                                &mut out,
                                line,
                                format!("duplicate style {name:?}; last definition wins"),
                            );
                        }
                    }
                    _ => diagnostic(&mut out, line, "style requires Name and Fontname"),
                }
            }
        } else if section == "[events]" {
            if kind == "format" {
                event_format = columns(value);
            }
            if kind == "dialogue" {
                if event_format.last().map(String::as_str) != Some("text") {
                    diagnostic(&mut out, line, "Text must be the last event field");
                    continue;
                }
                let values: Vec<_> = value.splitn(event_format.len(), ',').collect();
                if values.len() != event_format.len() {
                    diagnostic(&mut out, line, "dialogue field count differs from Format");
                    continue;
                }
                match (
                    field(&event_format, &values, "style"),
                    field(&event_format, &values, "text"),
                ) {
                    (Some(style), Some(text)) => events.push((line, style.trim(), text)),
                    _ => diagnostic(&mut out, line, "dialogue requires Style and Text"),
                }
            }
        }
    }
    let mut references = BTreeMap::new();
    for (line, style, text) in events {
        let base = styles.get(style).or_else(|| styles.get("Default"));
        if !styles.contains_key(style) {
            diagnostic(
                &mut out,
                line,
                format!("unknown style {style:?}; using Default if available"),
            );
        }
        let mut state = State::new(base);
        let mut remaining = text;
        while let Some(open) = remaining.find('{') {
            collect(&remaining[..open], &state, line, &mut references);
            let Some(close) = remaining[open + 1..].find('}') else {
                diagnostic(
                    &mut out,
                    line,
                    "unclosed override block; treated as literal text",
                );
                break;
            };
            let close = open + 1 + close;
            apply_tags(
                &remaining[open + 1..close],
                base,
                &styles,
                &mut state,
                line,
                &mut out,
            );
            remaining = &remaining[close + 1..];
        }
        collect(remaining, &state, line, &mut references);
    }
    out.references = references.into_values().collect();
    out.diagnostics.sort_by_key(|d| d.line);
    out
}

struct State<'a> {
    font: Option<&'a str>,
    style: Option<&'a Style>,
    weight: u16,
    italic: bool,
    drawing: bool,
}

impl<'a> State<'a> {
    fn new(style: Option<&'a Style>) -> Self {
        Self {
            font: style.map(|s| s.font.as_str()),
            style,
            weight: style.map_or(400, |s| s.weight),
            italic: style.is_some_and(|s| s.italic),
            drawing: false,
        }
    }
}

fn collect(
    text: &str,
    state: &State<'_>,
    line: usize,
    refs: &mut BTreeMap<(String, u16, bool), FontReference>,
) {
    if state.drawing {
        return;
    }
    let characters = rendered_characters(text);
    if characters.is_empty() {
        return;
    }
    if let Some(font) = state.font.filter(|s| !normalize_name(s).is_empty()) {
        let entry = refs
            .entry((normalize_name(font), state.weight, state.italic))
            .or_insert_with(|| FontReference {
                name: font.trim().into(),
                weight: state.weight,
                italic: state.italic,
                lines: Vec::new(),
                characters: Vec::new(),
            });
        if entry.lines.last() != Some(&line) {
            entry.lines.push(line);
        }
        for character in characters {
            match entry
                .characters
                .binary_search_by_key(&character, |usage| usage.character)
            {
                Ok(index) => {
                    let lines = &mut entry.characters[index].lines;
                    if lines.last() != Some(&line) {
                        lines.push(line);
                    }
                }
                Err(index) => entry.characters.insert(
                    index,
                    CharacterUsage {
                        character,
                        lines: vec![line],
                    },
                ),
            }
        }
    }
}

fn rendered_characters(text: &str) -> Vec<char> {
    let mut characters = Vec::new();
    let mut input = text.chars().peekable();
    while let Some(character) = input.next() {
        if character == '\\'
            && input
                .peek()
                .is_some_and(|next| matches!(next, 'N' | 'n' | 'h'))
        {
            input.next();
            continue;
        }
        if !character.is_whitespace() && !character.is_control() {
            match characters.binary_search(&character) {
                Ok(_) => {}
                Err(index) => characters.insert(index, character),
            }
        }
    }
    characters
}

fn apply_tags<'a>(
    block: &'a str,
    base: Option<&'a Style>,
    styles: &'a BTreeMap<String, Style>,
    state: &mut State<'a>,
    line: usize,
    out: &mut SubtitleFonts,
) {
    // Parenthesized tag arguments (including transforms and clips) are not
    // top-level overrides. Font name and reset are not animatable ASS tags.
    let mut starts = Vec::new();
    let mut depth = 0usize;
    for (i, ch) in block.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '\\' if depth == 0 => starts.push(i),
            _ => {}
        }
    }
    starts.push(block.len());
    for pair in starts.windows(2) {
        let tag = block[pair[0] + 1..pair[1]].trim();
        if let Some(name) = tag.strip_prefix("fn") {
            state.font = if name.trim().is_empty() {
                state.style.map(|s| s.font.as_str())
            } else {
                Some(name.trim())
            };
        } else if let Some(name) = tag.strip_prefix('r') {
            let name = name.trim();
            let style = if name.is_empty() {
                base
            } else {
                let font = styles.get(name);
                if font.is_none() {
                    diagnostic(
                        out,
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
            } else if let Some(weight) = weight(value) {
                state.weight = weight;
            } else {
                diagnostic(out, line, format!("invalid bold override {tag:?}; ignored"));
            }
        } else if let Some(value) = property(tag, 'i') {
            if value.is_empty() {
                state.italic = state.style.is_some_and(|s| s.italic);
            } else if let Some(italic) = italic(value) {
                state.italic = italic;
            } else {
                diagnostic(
                    out,
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
