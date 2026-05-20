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

Single-crate Rust CLI with internal modules:

```text
cli/command dispatch
ui output + prompt
engine scanner/github/rewrite/git
model shared internal types
syntax tree-sitter YAML parsing + GitHub Actions semantics
```

### Module Map

| Path | Role |
|---|---|
| `Cargo.toml` | Root package manifest and dependency versions |
| `src/main.rs` | CLI bootstrap and top-level dispatch |
| `src/cli.rs` | Clap parsing, global args, subcommands |
| `src/cmd/` | `update`, `validate`, `audit`, `version` handlers |
| `src/ui/output.rs` | Human-readable and JSON output |
| `src/ui/prompt.rs` | Interactive picker using `ratatui` + `crossterm` |
| `src/engine/scanner.rs` | Filesystem traversal and YAML file discovery |
| `src/engine/github/` | GitHub transport, resolution, tags, diagnostics |
| `src/engine/rewrite.rs` | Text-edit application and file rewriting |
| `src/engine/git.rs` | Version parsing and SHA helpers |
| `src/syntax/github_actions.rs` | Reference extraction from workflows/composite actions |
| `src/syntax/yaml_tree.rs` | tree-sitter YAML wrapper helpers |
| `src/model/` | `Reference`, `Candidate`, resolve/config/query types |

### Design Rules

- Keep terminal UI concerns in `ui/`.
- Keep scanner/resolver/rewrite logic in `engine/`.
- Keep shared internal types in `model/`.
- Keep the JSON output contract stable unless the change is deliberate and versioned.
- Tree-sitter is the parser/rewrite path; do not replace it with `serde_yaml` for workflow mutation.

## Conventions

- Prefer `rg` / `rg --files` for search.
- Do not add comments to code unless asked.
- Prefer behavior-preserving changes in parser, resolver, rewrite, output, and prompt code.
- Add or update tests with behavior-sensitive changes.
- Keep human output changes intentional; the CLI logs are part of the product surface.
