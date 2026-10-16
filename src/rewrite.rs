use crate::{
    Diagnostic, normalize_name,
    syntax::{self, Piece, Record, diagnostic, offset},
};

/// One font name occurrence that [`rewrite_font_names`] can replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontNameSite<'a> {
    /// Original trimmed name without its `@` prefix.
    pub name: &'a str,
    /// Whether the original spelling used the vertical `@` prefix.
    pub vertical: bool,
    /// Where the name occurs.
    pub kind: SiteKind<'a>,
    /// One-based source line number.
    pub line: usize,
}

/// Location of a [`FontNameSite`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SiteKind<'a> {
    /// The `Fontname` field of a style definition.
    StyleFontname {
        /// The style's `Name` field.
        style: &'a str,
    },
    /// A top-level `\fn` override in a `Dialogue:` or `Comment:` event.
    OverrideFn,
}

/// Output of [`rewrite_font_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    /// The rewritten subtitle text.
    pub text: String,
    /// Records that could not be parsed (and were therefore left unchanged),
    /// unclosed override blocks, and rejected replacements, sorted by line.
    pub diagnostics: Vec<Diagnostic>,
}

/// Replace font names in decoded ASS/SSA text.
///
/// `f` is called once per site in source order and returns the replacement
/// name, or `None` to keep the original. Sites use the same parser as
/// [`crate::extract_fonts`], so a consistent mapping yields the same references
/// under the new names. The scope differs from extraction in two ways, both to
/// leave no original name behind:
///
/// - every parsable `Style:` line is rewritten, including unused styles;
/// - `Comment:` events are rewritten like `Dialogue:` events.
///
/// As in extraction, `\fn` inside parenthesized arguments (such as `\t(...)`),
/// tags in unclosed override blocks, and records that cannot be parsed are not
/// sites. A `\fn` without a name is a reset, not a site.
///
/// The library keeps the `@` prefix: `\fn@Foo` becomes `\fn@New`. All other
/// bytes, including a BOM, mixed line endings, comments, and `[Fonts]` data, are
/// preserved, so returning `None` everywhere reproduces the input exactly.
///
/// A replacement is rejected with a diagnostic, keeping the original name, if it
/// normalizes to an empty name, has surrounding whitespace, starts with `@`,
/// contains control characters, or contains characters that would change the
/// record structure: `,` in a style, or `\`, `{`, `}`, `(`, `)` in an override.
///
/// ```
/// let source = "[V4+ Styles]\r\nFormat: Name, Fontname\r\nStyle: Default,Foo\r\n\
///               [Events]\r\nFormat: Style, Text\r\nDialogue: Default,{\\fn@Foo}x\r\n";
/// let rewritten = ass_fonts::rewrite_font_names(source, |site| {
///     (site.name == "Foo").then(|| "F1".to_string())
/// });
/// assert!(rewritten.diagnostics.is_empty());
/// assert!(rewritten.text.contains("Style: Default,F1\r\n"));
/// assert!(rewritten.text.contains(r"{\fn@F1}x"));
/// ```
pub fn rewrite_font_names(
    text: &str,
    mut f: impl FnMut(&FontNameSite) -> Option<String>,
) -> Rewritten {
    let mut diagnostics = Vec::new();
    let mut output = Output {
        source: text,
        text: String::with_capacity(text.len()),
        copied: 0,
    };
    for record in syntax::records(text, true, &mut diagnostics) {
        match record {
            Record::Style {
                line, name, font, ..
            } => output.site(
                font,
                line,
                SiteKind::StyleFontname { style: name },
                &[','],
                &mut f,
                &mut diagnostics,
            ),
            Record::Event { line, text, .. } => syntax::pieces(text, |piece| match piece {
                Piece::Tag(tag) => {
                    if let Some(name) = syntax::font_tag(tag) {
                        output.site(
                            name,
                            line,
                            SiteKind::OverrideFn,
                            &['\\', '{', '}', '(', ')'],
                            &mut f,
                            &mut diagnostics,
                        );
                    }
                }
                Piece::Unclosed => diagnostic(
                    &mut diagnostics,
                    line,
                    "unclosed override block; treated as literal text",
                ),
                Piece::Text(_) => {}
            }),
        }
    }
    output.text.push_str(&text[output.copied..]);
    diagnostics.sort_by_key(|d| d.line);
    Rewritten {
        text: output.text,
        diagnostics,
    }
}

struct Output<'a> {
    source: &'a str,
    text: String,
    copied: usize,
}

impl<'a> Output<'a> {
    fn site(
        &mut self,
        original: &'a str,
        line: usize,
        kind: SiteKind<'a>,
        forbidden: &[char],
        f: &mut impl FnMut(&FontNameSite) -> Option<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if normalize_name(original).is_empty() {
            return;
        }
        let site = FontNameSite {
            name: original.trim_start_matches('@').trim_start(),
            vertical: original.starts_with('@'),
            kind,
            line,
        };
        let Some(replacement) = f(&site) else {
            return;
        };
        let valid = !normalize_name(&replacement).is_empty()
            && replacement.trim() == replacement
            && !replacement.starts_with('@')
            && !replacement.contains(|c: char| c.is_control() || forbidden.contains(&c));
        if !valid {
            diagnostic(
                diagnostics,
                line,
                format!(
                    "invalid replacement {replacement:?} for font {:?}; kept original",
                    site.name
                ),
            );
            return;
        }
        let start = offset(self.source, original);
        self.text.push_str(&self.source[self.copied..start]);
        if site.vertical {
            self.text.push('@');
        }
        self.text.push_str(&replacement);
        self.copied = start + original.len();
    }
}
