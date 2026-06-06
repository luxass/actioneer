# AGENTS.md

## Project

**Actioneer** is a Rust CLI that scans GitHub Actions workflow files, resolves action references against the GitHub API, detects available updates, and applies them with either interactive or non-interactive selection.

## Build & Test

```sh
cargo build
cargo run -- <args>
cargo test
```

The binary name is `actioneer`.

## Architecture

Single-crate Rust CLI with flat module layout:

```text
cli  command dispatch and clap definitions
cmd  update, audit, version command handlers
```

### Module Map

| Path | Role |
|---|---|
| `src/main.rs` | CLI bootstrap and top-level dispatch |
| `src/cli.rs` | Clap parsing, global args, subcommands |
| `src/model.rs` | Action, Tag, Version, PinStyle, UpdateMode, ResolveConfig |
| `src/scan.rs` | Filesystem traversal and YAML action reference extraction |
| `src/github.rs` | GitHub API transport, tag resolution, disk cache, auth |
| `src/resolve.rs` | Version comparison, SHA matching, update detection |
| `src/rewrite.rs` | Text-edit computation and file rewriting |
| `src/display.rs` | Human-readable and JSON output |
| `src/prompt.rs` | Interactive picker using ratatui + crossterm |
| `src/cmd/` | `update`, `audit`, `version` handlers |

## Conventions

- Prefer `rg` for search.
- Do not add comments to code unless asked.
- Add or update tests with behavior-sensitive changes.
- Keep human output changes intentional.
- No extracted helper functions under 10 lines used in a single place.
