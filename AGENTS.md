# AGENTS.md

## Project

**Actioneer** is a Rust CLI that scans GitHub Actions workflow files, resolves action references against the GitHub API, detects available updates, and applies them with either interactive or non-interactive selection.

## Build & Test

```sh
cargo build
cargo run -- <args>
cargo test
cargo test --features e2e --test e2e   # opt-in e2e suite (hermetic, wiremock-backed)
```

The binary name is `actioneer`.

## Architecture

Single-crate Rust CLI grouped by workflow scanning, action resolution, GitHub transport, terminal output, and command handlers.

### Module Map

| Path | Role |
|---|---|
| `src/main.rs` | CLI bootstrap and top-level dispatch |
| `src/cli.rs` | Clap parsing, global args, subcommands |
| `src/actions/reference.rs` | Scan-owned action references, resolved action updates, update notes, workflow edit anchors |
| `src/actions/resolution.rs` | Tag model, pin/update config, update detection from discovered references |
| `src/actions/version.rs` | Version parsing, SHA detection, SHA matching |
| `src/workflows/discover.rs` | Filesystem traversal and YAML action reference extraction |
| `src/workflows/patch.rs` | Workflow file rewriting for selected resolved updates |
| `src/github/client.rs` | GitHub tag transport, pagination, auth token lookup |
| `src/github/cache.rs` | Disk cache path policy, cache reads/writes, cache disabling |
| `src/terminal/display.rs` | Human-readable printer helpers and JSON output |
| `src/terminal/prompt.rs` | Interactive picker using ratatui + crossterm |
| `src/cmd/mod.rs` | Shared command pipeline: input defaults, scan banner, discovery, tag fetching with error reporting |
| `src/cmd/` | `update`, `audit`, `version` command handlers |

### Data Flow

```text
workflows::discover -> ActionReference
actions::resolve -> ActionUpdate
cmd::update/audit -> terminal display, prompt, or workflows::patch
```

- `ActionReference` represents a GitHub Actions reference discovered in workflow YAML.
- `ActionUpdate` represents a resolved update or audit finding derived from an `ActionReference`.
- `WorkflowEdit` groups workflow rewrite coordinates and is skipped from JSON output.

## Conventions

- Prefer `rg` for search.
- Do not add comments to code unless asked.
- Add or update tests with behavior-sensitive changes.
- Keep human output changes intentional.
- No extracted helper functions under 10 lines used in a single place.
