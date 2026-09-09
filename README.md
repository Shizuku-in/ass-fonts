# ass-fonts

English | [简体中文](README-zh.md)

A Rust library that extracts fonts used in ASS/SSA dialogue, scans local font files, and matches references against internal font names.

- Tracks ASS/SSA styles, `\fn`, `\b`, `\i`, and `\r`; skips unused styles, comments, and drawing content.
- Scans TTF, OTF, TTC, and OTC, including every face in font collections.
- Returns `resolved` (one candidate), `missing` (none), or `ambiguous` (multiple), with source line numbers, file paths, and face indices.
- Provides parsing/scanning diagnostics and serializable reports.

## Usage

```rust
use ass_fonts::{read_subtitle, FontIndex, ScanReport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subtitle = read_subtitle("movie.ass")?;
    let scan = ScanReport::scan(["./fonts"]);
    let index = FontIndex::new(scan.faces);
    let report = index.resolve(&subtitle.references);

    println!("{report:#?}");
    // Check diagnostics for skipped or incomplete input.
    eprintln!("{:?}", subtitle.diagnostics);
    eprintln!("{:?}", scan.issues);
    Ok(())
}
```

Run the JSON report example:

```sh
cargo run --example report -- movie.ass ./fonts
```

To allow renderer-side style synthesis when collecting fonts (currently bold only):

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        synthesis: ass_fonts::SynthesisPolicy { bold: true },
        ..Default::default()
    },
);
```

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

`ResolveOptions` separates weight selection (`WeightMatching::Exact` by default, or `Nearest`) from synthesis (`bold`, off by default). `resolve` uses these defaults; `resolve_with_mode` remains a compatibility wrapper. Synthesis first prefers exact family matches, then permits a weight 400 face for a weight 700 request with the same italic state. Multiple eligible faces remain ambiguous. `matches[].synthetic_bold: true` marks candidates requiring emboldening. No font file is generated; rendering and visual quality are not verified.


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
  "reference": { "name": "Unknown Font", "weight": 400, "italic": false, "lines": [12] },
  "candidates": [],
  "missing_reason": "name_not_found",
  "matches": []
}
```

## Matching and scope

Matching uses internal family, full, PostScript, and related names, normalized with Unicode NFKC, lowercase conversion, whitespace normalization, and removal of the ASS vertical-font `@` prefix. Filenames are not used.

References are grouped by name, weight, and italic state. Normal/bold map to 400/700; explicit `\b100`–`\b900` weights are retained. PostScript names take priority. Full names identify specific faces unless they also denote a generic family. Legacy family aliases can identify variants when a broader typographic family is recorded and all matching faces share the same weight/italic attributes. This uses name-table relationships, not suffixes such as Bold or W17.

Generic families use the configured weight policy and require matching italic attributes (oblique counts as italic). Insufficient metadata keeps the conservative family interpretation. Specific names preserve the face's native design even when the request says 400; this resolves a font dependency, not the accuracy of the requested visual style. Requested and native attributes remain in the report. Duplicate matching files remain ambiguous. Italic synthesis is not yet implemented.

`read_subtitle` accepts UTF-8 and BOM-marked UTF-16 LE/BE. For legacy encodings, decode the text first and pass it to `extract_fonts`.

This library does not simulate rendering, check glyph coverage, generate synthesized fonts, instantiate variable fonts, resolve cross-family font fallback, extract embedded fonts, or discover system font directories. The optional synthetic-bold policy supports only 400→700, with no italic synthesis or Light fallback. Supply font paths explicitly and check diagnostics for incomplete results.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
