"""Build the crate and audit its archive without publishing or extracting it."""

import argparse
import json
from pathlib import Path, PurePosixPath
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-dirty", action="store_true", help="audit local uncommitted changes")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root,
    ))
    package = next(p for p in metadata["packages"] if Path(p["manifest_path"]) == root / "Cargo.toml")
    command = ["cargo", "package", "--locked"]
    if args.allow_dirty:
        command.append("--allow-dirty")
    subprocess.run(command, cwd=root, check=True)

    prefix = f'{package["name"]}-{package["version"]}'
    archive = Path(metadata["target_directory"]) / "package" / f"{prefix}.crate"
    required = {
        "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "README-zh.md",
        "CHANGELOG.md", "FUZZING.md", "RELEASING.md", "src/lib.rs", "src/coverage.rs",
        "src/fonts.rs", "src/subtitle.rs", "benches/pipeline.rs",
        "examples/report.rs", "examples/sample.ass",
    }
    allowed = required | {"Cargo.toml.orig", ".cargo_vcs_info.json"}
    seen = set()
    total = 0
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            path = PurePosixPath(member.name)
            if not member.isfile() or path.parts[0] != prefix or ".." in path.parts:
                raise SystemExit(f"Unexpected archive entry: {member.name}")
            relative = PurePosixPath(*path.parts[1:])
            name = str(relative)
            source = relative.parts[0] in {"src", "tests", "benches"} and relative.suffix == ".rs"
            if name not in allowed and not source:
                raise SystemExit(f"Unexpected packaged file: {name}")
            if name in seen:
                raise SystemExit(f"Duplicate packaged file: {name}")
            seen.add(name)
            total += member.size
    if required - seen:
        raise SystemExit(f"Missing release files: {sorted(required - seen)}")
    if not any(name.startswith("tests/") for name in seen):
        raise SystemExit("Package must include its self-contained Rust tests")
    if total > 1024 * 1024 or archive.stat().st_size > 512 * 1024:
        raise SystemExit("Package exceeded the source-only size budget; inspect its contents")
    print(f"Verified {archive}: {len(seen)} files, {total} bytes uncompressed, "
          f"{archive.stat().st_size} bytes compressed; no real-test data or font binaries")


if __name__ == "__main__":
    main()
