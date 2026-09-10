# Fuzzing

The fuzz package targets code owned by this crate: ASS/SSA extraction and font
index/resolution. Binary font parsing is delegated to `ttf-parser`, which maintains
its own fuzzing infrastructure.

Install the prerequisites once:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
```

Run either target from the repository root:

```sh
cargo +nightly fuzz run subtitle
cargo +nightly fuzz run resolver
```

Limit a local smoke run by time, for example:

```sh
cargo +nightly fuzz run subtitle -- -max_total_time=30
```

Generated corpora, crash artifacts, coverage data, and fuzz build output are ignored.
Keep small, intentional regression inputs under `fuzz/corpus/`; promote fixed crashes
to regular integration tests when practical.

Regular CI compiles and lints both targets. A separate workflow runs each target
under libFuzzer and sanitizer instrumentation for 30 seconds every Monday and can
also be started manually.
