//! Line and override-tag tokenizer shared by extraction and rewriting.
//!
//! Every returned `&str` is a subslice of the input, so callers can recover
//! byte ranges with [`offset`] and edit the original text in place.

use crate::{Diagnostic, normalize_name};

/// Byte offset of `part` within `base`; `part` must be a subslice of `base`.
pub(crate) fn offset(base: &str, part: &str) -> usize {
    part.as_ptr() as usize - base.as_ptr() as usize
}

pub(crate) enum Record<'a> {
    Style {
        line: usize,
        name: &'a str,
        /// Trimmed Fontname field, possibly with `@` prefixes.
        font: &'a str,
        weight: u16,
        italic: bool,
    },
    Event {
        line: usize,
        style: &'a str,
        /// Untrimmed Text field.
        text: &'a str,
    },
}

pub(crate) fn weight(value: &str) -> Option<u16> {
    match value.trim().parse::<i32>().ok()? {
        0 => Some(400),
        -1 | 1 => Some(700),
        n @ 100..=900 => Some(n as u16),
        _ => None,
    }
}

pub(crate) fn italic(value: &str) -> Option<bool> {
    match value.trim().parse::<i32>().ok()? {
        0 => Some(false),
        -1 | 1 => Some(true),
        _ => None,
    }
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

pub(crate) fn diagnostic(out: &mut Vec<Diagnostic>, line: usize, message: impl Into<String>) {
    out.push(Diagnostic {
        line,
        message: message.into(),
    });
}

/// Style and event records in source order.
///
/// Lines are split like [`str::lines`], but through `split_inclusive` so that
/// each record keeps its position in the original text. `Comment:` events are
/// returned (and diagnosed) only when `comments` is set.
pub(crate) fn records<'a>(
    text: &'a str,
    comments: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Record<'a>> {
    let mut records = Vec::new();
    let mut section = String::new();
    let mut style_format = Vec::new();
    let mut event_format =
        columns("Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text");
    let body = text.trim_start_matches('\u{feff}');
    for (index, raw) in body.split_inclusive('\n').enumerate() {
        let line = index + 1;
        let raw = raw
            .strip_suffix('\n')
            .map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l))
            .trim();
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
                    diagnostic(diagnostics, line, "style field count differs from Format");
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
                                diagnostics,
                                line,
                                "invalid Bold/Italic; invalid properties use normal defaults",
                            );
                        }
                        records.push(Record::Style {
                            line,
                            name,
                            font,
                            weight: weight(bold).unwrap_or(400),
                            italic: italic(slant).unwrap_or(false),
                        });
                    }
                    _ => diagnostic(diagnostics, line, "style requires Name and Fontname"),
                }
            }
        } else if section == "[events]" {
            if kind == "format" {
                event_format = columns(value);
            }
            if kind == "dialogue" || (comments && kind == "comment") {
                if event_format.last().map(String::as_str) != Some("text") {
                    diagnostic(diagnostics, line, "Text must be the last event field");
                    continue;
                }
                let values: Vec<_> = value.splitn(event_format.len(), ',').collect();
                if values.len() != event_format.len() {
                    diagnostic(
                        diagnostics,
                        line,
                        "dialogue field count differs from Format",
                    );
                    continue;
                }
                match (
                    field(&event_format, &values, "style"),
                    field(&event_format, &values, "text"),
                ) {
                    (Some(style), Some(text)) => records.push(Record::Event {
                        line,
                        style: style.trim(),
                        text,
                    }),
                    _ => diagnostic(diagnostics, line, "dialogue requires Style and Text"),
                }
            }
        }
    }
    records
}

pub(crate) enum Piece<'a> {
    /// Literal text outside override blocks.
    Text(&'a str),
    /// One trimmed top-level tag without its leading backslash.
    Tag(&'a str),
    /// An override block without `}`; the rest follows as [`Piece::Text`].
    Unclosed,
}

/// Split event text into literal runs and top-level override tags.
///
/// Parenthesized tag arguments (including transforms and clips) are not
/// top-level overrides; they stay inside the enclosing tag.
pub(crate) fn pieces<'a>(text: &'a str, mut f: impl FnMut(Piece<'a>)) {
    let mut remaining = text;
    while let Some(open) = remaining.find('{') {
        f(Piece::Text(&remaining[..open]));
        let Some(close) = remaining[open + 1..].find('}') else {
            f(Piece::Unclosed);
            f(Piece::Text(&remaining[open..]));
            return;
        };
        let close = open + 1 + close;
        let block = &remaining[open + 1..close];
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
            f(Piece::Tag(block[pair[0] + 1..pair[1]].trim()));
        }
        remaining = &remaining[close + 1..];
    }
    f(Piece::Text(remaining));
}

/// The trimmed font name of a `\fn` tag; an empty name resets to the style font.
pub(crate) fn font_tag(tag: &str) -> Option<&str> {
    tag.strip_prefix("fn").map(str::trim)
}
