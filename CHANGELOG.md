# Changelog

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
