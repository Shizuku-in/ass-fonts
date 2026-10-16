# ass-fonts

English | [简体中文](README-zh.md)

A Rust library that extracts fonts used in ASS/SSA dialogue, scans local font files, and matches references against internal font names.

- Tracks ASS/SSA styles, `\fn`, `\b`, `\i`, and `\r`; skips unused styles, comments, and drawing content.
- Records distinct dialogue characters, whitespace, and their source lines for each font request, and whether it was used vertically (`@`).
- Rewrites font names in a subtitle with the same parser, preserving every other byte.
- Scans TTF, OTF, TTC, and OTC, including every face in font collections and legacy CJK name records.
- Supports cached scans (`discover_font_files`, `scan_font_file`), progress callbacks, and optional parallel scanning.
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

To **rename fonts** in a subtitle, for example before muxing subset fonts:

```rust
let rewritten = ass_fonts::rewrite_font_names(&text, |site| {
    // Return None to keep a name; `@` prefixes are kept automatically.
    Some(format!("{:08X}", hash(site.name)))
});
```

## Features

| Feature | Default | Purpose |
| --- | --- | --- |
| `legacy-names` | yes | Decode Windows/Macintosh CJK legacy-encoded name records |
| `encoding` | no | `decode_subtitle`: detect GBK, Big5, Shift_JIS, and other legacy subtitle encodings (via `chardetng`); `read_subtitle` uses it too |
| `parallel` | no | Scan font files in parallel with rayon; results equal a serial scan |

## Matching and scope

- Matches internal font names, ignoring case, whitespace, and the vertical `@` prefix. Filenames are not used.
- Names that identify a specific face keep its native weight/italic; family names are matched by the requested weight and italic.
- Bold (400→700) and italic synthesis are off by default; enable them with `SynthesisPolicy`.
- Without the `encoding` feature, `read_subtitle` accepts UTF-8 and BOM-marked UTF-16. Decode other encodings yourself and pass the text to `extract_fonts`.
- Coverage checks ignore whitespace unless `CoverageOptions::include_whitespace` is set.

## Development

```sh
cargo test --locked --all-features
cargo clippy --locked --all-targets -- -D warnings
cargo bench --locked --bench pipeline
```

- Fuzzing: [FUZZING.md](FUZZING.md)
- Changelog: [CHANGELOG.md](CHANGELOG.md)

# LICENSE

[MIT](./LICENSE)