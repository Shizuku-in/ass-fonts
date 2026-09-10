# ass-fonts

English | [简体中文](README-zh.md)

A Rust library that extracts fonts used in ASS/SSA dialogue, scans local font files, and matches references against internal font names.

Requires Rust **1.87+**. Licensed under [MIT](LICENSE).

- Tracks ASS/SSA styles, `\fn`, `\b`, `\i`, and `\r`; skips unused styles, comments, and drawing content.
- Records distinct dialogue characters and their source lines for each font request.
- Scans TTF, OTF, TTC, and OTC, including every face in font collections.
- Returns `resolved` (one candidate), `missing` (none), or `ambiguous` (multiple), with source line numbers, file paths, and face indices.
- Optionally reports nominal cmap coverage as `complete`, `incomplete`, or `uncheckable`.
- Provides parsing/scanning diagnostics and serializable reports.

## Usage

```rust
use ass_fonts::{read_subtitle, FontIndex, ScanReport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subtitle = read_subtitle("movie.ass")?;
    let scan = ScanReport::scan(["./fonts"]);
    let index = FontIndex::new(scan.faces);
    let report = index.resolve(&subtitle.references);
    let coverage = report.check_coverage();

    println!("{report:#?}");
    println!("{coverage:#?}");
    // Check diagnostics for skipped or incomplete input.
    eprintln!("{:?}", subtitle.diagnostics);
    eprintln!("{:?}", scan.issues);
    Ok(())
}
```

Run the JSON report example:

```sh
cargo run --example report -- movie.ass ./fonts
cargo run --example report -- --check-coverage movie.ass ./fonts
```

`check_coverage` reopens each selected font file and checks the distinct characters assigned to that request. Ambiguous resolutions are checked once per candidate, so one candidate may be complete while another is incomplete. Missing resolutions and files/faces that can no longer be read or parsed are `uncheckable`. The check uses nominal Unicode cmap mappings only: it does not shape or render text, apply cross-family fallback, or verify variation sequences and visual quality.

Use `coverage.summary()` for candidate and missing-character counts, `coverage.is_complete()` for a strict success check, and `coverage.missing_characters()` for a deduplicated list with merged source lines. `CharacterUsage::codepoint()` formats a value such as `U+5B57`.

For compact JSON and CI checks:

```sh
cargo run --example report -- --summary movie.ass ./fonts
cargo run --example report -- --summary \
  --fail-on-missing-font --fail-on-ambiguous-font \
  --fail-on-missing-glyph --fail-on-uncheckable movie.ass ./fonts
```

`--summary` automatically checks coverage and includes aggregated missing characters with code points and lines. Failure policies are opt-in: argument errors exit with code 2, requested font matching failures with 3, missing glyphs with 4, and uncheckable coverage with 5. Other runtime errors use code 1. If several enabled conditions occur, the order is font matching, missing glyphs, then uncheckable coverage.

To allow renderer-side style synthesis when collecting fonts (bold and italic):

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        synthesis: ass_fonts::SynthesisPolicy { bold: true, italic: true },
        ..Default::default()
    },
);
```

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

`ResolveOptions` separates weight selection (`WeightMatching::Exact` by default, or `Nearest`) from synthesis (`bold` and `italic`, both off by default). `resolve` uses these defaults; `resolve_with_mode` remains a compatibility wrapper. Selection first tries native slant with exact weight, optional nearest weight, then permitted 400→700 bold fallback. If no candidate remains, italic permission allows an italic request to repeat weight selection among upright faces. Upright requests never fall back to italic faces. Multiple eligible faces remain ambiguous. `matches[].synthetic_bold` and `synthetic_italic` independently mark required synthesis. The CLI `--allow-style-synthesis` and `ResolveMode::AllowStyleSynthesis` enable both; use `SynthesisPolicy` to allow only one. No font file is generated; rendering and visual quality are not verified.


To enable nearest-weight selection independently of synthesis:

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        weight_matching: ass_fonts::WeightMatching::Nearest,
        ..Default::default()
    },
);
```

```sh
cargo run --example report -- --nearest-weight movie.ass ./fonts
cargo run --example report -- --nearest-weight --allow-style-synthesis movie.ass ./fonts
```

`Nearest` prefers exact matches, otherwise minimizes absolute weight difference within the same family and italic state. Equal distances and duplicate faces remain ambiguous; there is no distance cutoff. It does not alter specific-name selection. Selection happens before the independent synthesis decision: 700→693 is `family_nearest` without synthetic bold; 700→400 gets synthetic bold only when explicitly enabled. The JSON example reports the full `options` object instead of the former `mode` field.

Each entry includes `candidates` and `matches` (one provenance record per candidate). Missing entries have empty candidates and matches, plus:

| `missing_reason` | Meaning | `available_variants` |
| --- | --- | --- |
| `name_not_found` | No internal name matched | Omitted |
| `style_not_found` | Family found, requested weight/italic unavailable | All scanned faces matching that family |

`matches[].matched_names` contains the original internal names and their `kind`: `family`, `full_name`, or `post_script_name`. Caller-supplied names without type metadata use `internal_name`. Available variants are informational, not fallback selections. `missing_reason` is omitted for successful and ambiguous entries.

Scanned names retain their original `name_id`. Each match has a `selection_method`: `post_script_name`, `full_name`, `legacy_family_name`, `family_exact`, `family_nearest`, `family_synthesis`, or `internal_name`.

For example, a missing-name entry in the JSON report is:

```json
{
  "reference": {
    "name": "Unknown Font", "weight": 400, "italic": false,
    "lines": [12], "characters": [{ "character": "A", "lines": [12] }]
  },
  "candidates": [],
  "missing_reason": "name_not_found",
  "matches": []
}
```

## Matching and scope

Matching uses internal family, full, PostScript, and related names, normalized with Unicode NFKC, lowercase conversion, whitespace normalization, and removal of the ASS vertical-font `@` prefix. Filenames are not used.

References are grouped by name, weight, and italic state. Normal/bold map to 400/700; explicit `\b100`–`\b900` weights are retained. PostScript names take priority. Full names identify specific faces unless they also denote a generic family. Legacy family aliases can identify variants when a broader typographic family is recorded and all matching faces share the same weight/italic attributes. This uses name-table relationships, not suffixes such as Bold or W17.

Generic families use the configured weight policy and prefer matching italic attributes (oblique counts as italic). Insufficient metadata keeps the conservative family interpretation. Specific names preserve the face's native design even when the request says 400; this resolves a font dependency, not the accuracy of the requested visual style. Requested and native attributes remain in the report. Duplicate matching files remain ambiguous. Italic synthesis requires explicit permission.

`read_subtitle` accepts UTF-8 and BOM-marked UTF-16 LE/BE. For legacy encodings, decode the text first and pass it to `extract_fonts`.

This library can check nominal cmap coverage, but does not simulate shaping/rendering, generate synthesized fonts, instantiate variable fonts, resolve cross-family font fallback, extract embedded fonts, or discover system font directories. Synthetic bold supports only 400→700; italic synthesis permits upright→italic. These permissions may combine and do not alter font files. Light faces are not used for synthetic bold. Supply font paths explicitly and check diagnostics for incomplete results.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

See [release preparation](RELEASING.md) for the CI matrix, MSRV checks, and package audit, and [CHANGELOG.md](CHANGELOG.md) for API/JSON behavior notes. Local `real-test` fonts and subtitles are excluded from the crate package.
