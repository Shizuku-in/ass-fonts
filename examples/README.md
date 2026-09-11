# Example

English | [简体中文](README-zh.md)

## JSON report

```sh
cargo run --example report -- movie.ass ./fonts
cargo run --example report -- --check-coverage movie.ass ./fonts
```

`check_coverage` reopens each selected font file and checks the distinct characters assigned to that request. Ambiguous resolutions are checked once per candidate, so one candidate may be complete while another is incomplete. Missing resolutions and files/faces that can no longer be read or parsed are `uncheckable`. The check uses nominal Unicode cmap mappings only: it does not shape or render text, apply cross-family fallback, or verify variation sequences and visual quality.

Use `coverage.summary()` for candidate and missing-character counts, `coverage.is_complete()` for a strict success check, and `coverage.missing_characters()` for a deduplicated list with merged source lines. `CharacterUsage::codepoint()` formats a value such as `U+5B57`.

## Compact JSON & CI checks

For compact JSON and CI checks:

```sh
cargo run --example report -- --summary movie.ass ./fonts
cargo run --example report -- --summary \
  --fail-on-missing-font --fail-on-ambiguous-font \
  --fail-on-missing-glyph --fail-on-uncheckable movie.ass ./fonts
```

`--summary` automatically checks coverage and includes aggregated missing characters with code points and lines. Failure policies are opt-in: argument errors exit with code 2, requested font matching failures with 3, missing glyphs with 4, and uncheckable coverage with 5. Other runtime errors use code 1. If several enabled conditions occur, the order is font matching, missing glyphs, then uncheckable coverage.

## Allow renderer-side style synthesis

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

`ResolveOptions` separates weight selection (`WeightMatching::Exact` by default, or `Nearest`) from synthesis (`bold` and `italic`, both off by default). `resolve` uses these defaults; `resolve_with_mode` remains a compatibility wrapper. Selection first tries native slant with exact weight, optional nearest weight, then permitted 400→700 bold fallback. If no candidate remains, italic permission allows an italic request to repeat weight selection among upright faces. Upright requests never fall back to italic faces. Multiple eligible faces remain ambiguous. `matches[].synthetic_bold` and `synthetic_italic` independently mark required synthesis. The CLI `--allow-style-synthesis` and `ResolveMode::AllowStyleSynthesis` enable both; use `SynthesisPolicy` to allow only one. No font file is generated; rendering and visual quality are not verified.

## Nearest-weight selection

```sh
cargo run --example report -- --nearest-weight movie.ass ./fonts
cargo run --example report -- --nearest-weight --allow-style-synthesis movie.ass ./fonts
```

`Nearest` prefers exact matches, otherwise minimizes absolute weight difference within the same family and italic state. Equal distances and duplicate faces remain ambiguous; there is no distance cutoff. It does not alter specific-name selection. Selection happens before the independent synthesis decision: 700→693 is `family_nearest` without synthetic bold; 700→400 gets synthetic bold only when explicitly enabled. The JSON example reports the full `options` object instead of the former `mode` field.

## Result Semantics

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