# Release preparation

The crate is MIT-licensed. The supported minimum Rust version (MSRV) is **1.87.0** for the library, examples, and tests. The test fixtures use integer `is_multiple_of`, stabilized in 1.87. Dependency updates must continue to pass the MSRV job with the committed `Cargo.lock`; older compilers are not part of the supported matrix.

## Local checks

```sh
rustup toolchain install 1.87.0 --profile minimal
cargo +1.87.0 test --locked --all-targets
cargo +1.87.0 test --locked --doc
cargo +stable fmt --check
cargo +stable clippy --locked --all-targets -- -D warnings
cargo +stable test --locked --all-targets
cargo +stable test --locked --doc
RUSTDOCFLAGS='-D warnings -D missing_docs' cargo +stable doc --locked --no-deps
python3 scripts/check-package.py
```

The `RUSTDOCFLAGS` assignment shown above uses POSIX shell syntax. The packaging script needs Python 3 and Cargo; no Python packages are required. For a local preview before committing, pass `--allow-dirty`. The final release check should use a clean worktree.

The script runs `cargo package` including Cargo's verification build, then inspects the actual archive. It checks required documentation, license, examples, and tests, rejects unapproved paths and font binaries, and enforces size limits. `Cargo.toml`'s explicit `include` list excludes `real-test` regardless of Git tracking or ignore rules. The synthetic font fixtures in Rust tests do not require or redistribute third-party fonts.

## Before publishing

1. Confirm stable CI passes on Linux, Windows, and macOS and the MSRV job passes on Linux.
2. Review `CHANGELOG.md`, API/JSON changes, and the version in `Cargo.toml`. Update the lockfile if the version or dependencies change.
3. Run the clean-worktree archive audit and inspect `cargo package --locked --list`.
4. Review the package name, registry access, release date, and release notes before performing a separate publication step.

CI only tests and packages; it does not publish to crates.io, create tags, or create GitHub releases. Real-data reports under `target/` are local validation artifacts, not release fixtures. `resolved` does not guarantee glyph coverage, and a complete nominal cmap check does not guarantee shaping or rendering fidelity; preserve these distinctions in release notes.
