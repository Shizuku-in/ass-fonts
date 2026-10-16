# Changelog

## 0.3.0 - 2026-10-16

### Added

- `rewrite_font_names` replaces style `Fontname` fields and top-level `\fn` names, including in unused styles and `Comment:` events, via a callback that receives each `FontNameSite`. `@` prefixes are kept and all other bytes are preserved; invalid replacements are rejected with diagnostics.
- `FontReference::whitespace` records whitespace per font request: `\h` as U+00A0 and `\n` as U+0020. `check_coverage_with` with `CoverageOptions { include_whitespace: true }` also checks it.
- `FontReference::vertical` reports whether visible text used the font through an `@` name.
- `legacy-names` feature (default): decode Windows ShiftJIS/PRC/Big5/Wansung and Macintosh Roman/Japanese/Traditional Chinese/Korean/Simplified Chinese name records. Undecodable records, including Johab, are skipped with a `ScanIssue`.
- `encoding` feature: `decode_subtitle` honors BOMs, then UTF-8, then detects legacy encodings with `chardetng`; `read_subtitle` uses it.
- Scan caching: `discover_font_files`, `scan_font_file`, `SCAN_FORMAT_VERSION`, and `Deserialize` for `FontFace`, `FontName`, `NameKind`, `ScanIssue`, and `ScanReport`.
- `ScanReport::scan_with` reports `ScanProgress` after each file. The `parallel` feature scans files with rayon and produces the same report as a serial scan.
- `FontIndex::faces` and `FontIndex::lookup`. Documented and tested alias injection through `FontIndex::new` merging.
- `Default` for `FontReference`; `PartialEq`/`Eq` for `Diagnostic`, `ScanIssue`, and `ScanReport`.

### Breaking changes

- `FontReference` has new `vertical` and `whitespace` fields; struct literals need them or `..Default::default()`, and serialized output changes.
- With the `encoding` feature, `read_subtitle` decodes legacy bytes instead of rejecting them.
- With `legacy-names`, scanning can return more names and more issues for the same files.

### Development

- CI tests and lints all feature combinations and fuzzes font name rewriting.
- Replace the Python package audit script with an equivalent shell step in CI.

## 0.2.3 - 2026-10-15

### Documentation

- Condense the README matching and scope notes and sync the English and Chinese versions.
- Remove `RELEASING.md` from the repository and the crate package.

### Development

- Share one synthetic font builder across integration tests and merge overlapping synthesis tests.

## 0.2.2 - 2026-09-10

### Development

- Extend Criterion coverage with warm scanning of 100 synthetic TTF files and cmap checks for 100 references sharing one or ten font faces.

## 0.2.1 - 2026-09-10

### Development

- Add reproducible Criterion benchmarks for large subtitle extraction, font indexing, and exact/nearest resolution.
- Add cargo-fuzz targets for ASS/SSA extraction and arbitrary font index/resolution inputs, with a seed corpus, usage guide, and scheduled sanitizer smoke runs.

## 0.2.0 - 2026-09-10

### Added

- Extract distinct dialogue characters per font request with source-line provenance.
- Check each resolved or ambiguous candidate's nominal cmap coverage with `check_coverage` or `ResolveReport::check_coverage`.
- Report coverage as `complete`, `incomplete`, or `uncheckable`; distinguish missing candidates, file read failures, and face parse failures.
- Add `CoverageReport::summary`, `is_complete`, and aggregated `missing_characters`, plus `CharacterUsage::codepoint` for display.
- Add `--check-coverage`, compact `--summary`, and opt-in CI failure policies to the JSON report example.

### Breaking changes

- `FontReference` now requires a `characters: Vec<CharacterUsage>` field; manual struct construction and serialized output therefore change.

### Limitations

- Coverage checks nominal Unicode cmap mappings only. They do not shape or render text, apply fallback, or verify variation sequences and visual quality.

## 0.1.0 - 2026-09-10

### Features

- Extract actual ASS/SSA dialogue font requests, including font, weight, italic, reset, and drawing tags.
- Scan TTF/OTF/TTC/OTC using internal names; retain collection face indices and name-table provenance.
- Report resolved, missing, and ambiguous dependencies with source lines, selection evidence, and available variants.
- Support exact or nearest family weights and independently permitted bold/italic synthesis.
- Provide English/Chinese documentation, a JSON report example, and MIT licensing.

### API and behavior notes

- `FontIndex::resolve` defaults to exact family attributes with synthesis disabled. It does not imply renderer-identical font selection.
- Prefer `resolve_with_options` with `ResolveOptions { weight_matching, synthesis }`. `resolve_with_mode` remains a compatibility wrapper; `AllowStyleSynthesis` enables both bold and italic with exact weight selection.
- `Nearest` minimizes absolute weight difference in the eligible family/slant tier. Equal-ranked faces remain ambiguous. Selection does not implicitly permit synthesis.
- Specific PostScript/full/legacy variant names can locate native faces even when the requested weight differs. Generic families remain subject to attribute selection.
- References are grouped by normalized name, weight, and italic state. Counts represent requests, not unique files.
- Public metadata includes `FontReference::weight/italic`, `FontFace::name_records`, and `FontName::name_id`. Constructing these structs requires the corresponding fields.
- `SynthesisPolicy` now includes both `bold` and `italic`; use `..Default::default()` when enabling only one.
- Match evidence contains `selection_method`, `synthetic_bold`, and `synthetic_italic`. Missing reasons serialize as `name_not_found` or `style_not_found`.
- The JSON example emits `options` instead of its earlier `mode` field. Enum values serialize in snake_case.
- `resolved` means a font dependency was located, not that glyph coverage, synthesis quality, or visual fidelity was verified. Check subtitle diagnostics and scan issues separately.

### Build and packaging

- Supported minimum Rust version: 1.87.0, including examples and tests with the committed lockfile.
- CI tests stable Rust on Linux, Windows, and macOS, plus Rust 1.87.0 on Linux; checks formatting, Clippy, Rustdoc, and release archives.
- Published files use an explicit manifest allowlist. Local `real-test` data, font binaries, generated reports, and CI tooling are excluded.
