# actioneer

`actioneer` scans GitHub Actions workflows, finds outdated `uses:` references, and can rewrite them to newer versions or pinned SHAs.

## Install

### Homebrew (macOS and Linux)

```sh
brew install luxass/homebrew-tap/actioneer
```

### Cargo

```sh
cargo install --locked actioneer
```

### GitHub Releases

Pre-built binaries are available for Linux and macOS at [github.com/luxass/actioneer/releases](https://github.com/luxass/actioneer/releases).

## Quick Start

```sh
actioneer --dry-run
actioneer --yes
actioneer audit
```

By default, `actioneer` scans `.github`. Use `--recursive` to scan from the current directory, or pass a file or directory explicitly.

## What It Does

- Finds GitHub Actions references in workflow YAML.
- Resolves newer tags through the GitHub API.
- Rewrites references either as SHAs or preserved tags.
- Detects SHA/comment mismatches before you trust pinned actions.
- Supports interactive use, CI validation, and JSON output.

## Notes

- Updates are currently rewritten as pinned SHAs with version comments.
- `audit` exits non-zero on SHA/comment mismatches.
- Interactive selection requires a TTY.
- Set `GITHUB_TOKEN` if you want higher GitHub API rate limits.
- Workflow security analysis runs in CI via `zizmor`.

## Workflow Security Checks

This repository uses [`zizmor`](https://github.com/zizmorcore/zizmor) to statically analyze GitHub Actions workflows.
`zizmor` itself is a Rust tool, and the upstream project ships both a Cargo-installable CLI and a GitHub Action wrapper.

For local use, install it with Cargo:

```sh
cargo install --locked zizmor
just zizmor .
```

The CI integration lives in `.github/workflows/zizmor.yaml` and uploads results through GitHub code scanning.

## Build From Source

Build the Rust CLI directly:

```sh
cargo build
./target/debug/actioneer --help
```

For local iteration:

```sh
cargo run -- --dry-run
cargo test
```

## 📄 License

Published under [MIT License](./LICENSE).
