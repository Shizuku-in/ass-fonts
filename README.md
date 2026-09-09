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

Each entry includes `candidates` and `matches` (one provenance record per candidate). Missing entries have empty candidates and matches, plus:

| `missing_reason` | Meaning | `available_variants` |
| --- | --- | --- |
| `name_not_found` | No internal name matched | Omitted |
| `style_not_found` | Family found, requested weight/italic unavailable | All scanned faces matching that family |

`matches[].matched_names` contains the original internal names and their `kind`: `family`, `full_name`, or `post_script_name`. Caller-supplied names without type metadata use `internal_name`. Available variants are informational, not fallback selections. `missing_reason` is omitted for successful and ambiguous entries.

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

References are grouped by name, weight, and italic state. Normal/bold map to 400/700; explicit `\b100`–`\b900` weights are retained. Family-name matches require exact weight and italic attributes (oblique counts as italic). No matching variant means `missing`; duplicate matching faces remain `ambiguous`. Full/PostScript names identify specific faces regardless of requested styling. If a name is also a family alias, family matching takes precedence.

`read_subtitle` accepts UTF-8 and BOM-marked UTF-16 LE/BE. For legacy encodings, decode the text first and pass it to `extract_fonts`.

This library does not simulate rendering, check glyph coverage, synthesize styles, choose the nearest weight, instantiate variable fonts, resolve font fallback, extract embedded fonts, or discover system font directories. Supply font paths explicitly and check diagnostics for incomplete results.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
