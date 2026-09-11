# ass-fonts

English | [简体中文](README-zh.md)

A Rust library that extracts fonts used in ASS/SSA dialogue, scans local font files, and matches references against internal font names.

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

To allow **renderer-side style synthesis** when collecting fonts (bold and italic):

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        synthesis: ass_fonts::SynthesisPolicy { bold: true, italic: true },
        ..Default::default()
    },
);
```

To enable **nearest-weight selection** independently of synthesis:

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        weight_matching: ass_fonts::WeightMatching::Nearest,
        ..Default::default()
    },
);
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
cargo bench --locked --bench pipeline
```

The benchmarks use generated, redistributable fixtures. Font scanning measures a warmed operating-system file cache; it is not a cold-disk benchmark.

See [fuzzing](FUZZING.md) for the subtitle and resolver targets, [release preparation](RELEASING.md) for the CI matrix, MSRV checks, and package audit, and [CHANGELOG.md](CHANGELOG.md) for API/JSON behavior notes. Local `real-test` fonts and subtitles are excluded from the crate package.

# LICENSE

[MIT](./LICENSE)