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

- Matches internal font names, ignoring case, whitespace, and the vertical `@` prefix. Filenames are not used.
- Names that identify a specific face keep its native weight/italic; family names are matched by the requested weight and italic.
- Bold (400→700) and italic synthesis are off by default; enable them with `SynthesisPolicy`.
- `read_subtitle` accepts UTF-8 and BOM-marked UTF-16. Decode other encodings yourself and pass the text to `extract_fonts`.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo bench --locked --bench pipeline
```

- Fuzzing: [FUZZING.md](FUZZING.md)
- Changelog: [CHANGELOG.md](CHANGELOG.md)

# LICENSE

[MIT](./LICENSE)