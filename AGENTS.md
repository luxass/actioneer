# AGENTS.md

## Project

**Actioneer** — a CLI tool that scans GitHub Actions workflow files, resolves action references against the GitHub API, detects available updates, and applies them (with interactive selection). Written in Zig against 0.16.0.

## Build & Test

```sh
zig build           # compile debug binary → zig-out/bin/actioneer
zig build run -- <args>  # build and run with arguments
zig build test      # compile and run all unit tests
```

No linter or formatter is configured.

## Architecture

Layered bottom-up:

```
syntax/          YAML parsing via tree-sitter, GitHub Actions semantics
core/            Business logic — scanning, GitHub API, rewriting, git helpers
app/             Orchestration (scan→resolve, rewrite) + UI (output, prompt, styles)
cmd/             CLI subcommand handlers (update, validate, version)
cli.zig          CLI tree construction, flag definitions, input parsing
main.zig         Entry point — I/O setup, CLI execution
```

### Module Map

| Path | Role | Lines |
|---|---|---|
| `src/main.zig` | Entry point: I/O buffers, CLI build, execute | 38 |
| `src/cli.zig` | `zli.Command` tree, all flags, `CommandInput` parsing, `ProcessState` | 287 |
| `src/cmd/update.zig` | Default subcommand: full scan→resolve→prompt→apply pipeline | 101 |
| `src/cmd/validate.zig` | `validate` subcommand: scan→resolve, exit non-zero on SHA mismatches | 68 |
| `src/cmd/version.zig` | `version` subcommand: prints app version | 13 |
| `src/app/check_workflows.zig` | Thin orchestration: scan + GitHub resolve with error handling + diagnostics | 93 |
| `src/app/apply_updates.zig` | Thin orchestration: rewrite wrapper with error handling | 38 |
| `src/app/ui/output.zig` | All human + JSON output, error messages, SHA mismatch helpers | 346 |
| `src/app/ui/prompt.zig` | Interactive terminal selection TUI (posix-only) | 424 |
| `src/app/ui/styles.zig` | ANSI escape constants (colors, cursor) — zero dependencies | 48 |
| `src/core/scanner.zig` | Filesystem traversal, YAML file detection, delegates to syntax layer | 130 |
| `src/core/github.zig` | GitHub API client, tag resolution, `Candidate` type, `Diagnostics` | 400 |
| `src/core/rewrite.zig` | File read/edit/write with `TextEdit`-based in-place replacement | 316 |
| `src/core/git.zig` | Semantic version parsing, SHA detection | 72 |
| `src/core/log.zig` | Colored debug/info/warn/error to stderr | 32 |
| `src/syntax/github_actions.zig` | GitHub Actions semantic parsing: `Reference`, `ActionName`, `ReferenceKind` | 533 |
| `src/syntax/yaml_tree.zig` | Tree-sitter YAML: parse, navigate, extract scalars/comments | 209 |
| `src/tests.zig` | Test aggregation — re-exports all modules with embedded `test` blocks | 9 |

### Dependency Graph

```
main.zig → cli.zig → cmd/{update,validate,version}.zig
                    → app/ui/output.zig
                    → core/github.zig (UpdateMode, PinStyle)
                    → core/log.zig

cmd/update.zig → cli.zig (cycle)
               → app/check_workflows.zig
               → app/apply_updates.zig
               → app/ui/{prompt,output}.zig
               → core/log.zig

cmd/validate.zig → cli.zig (cycle)
                 → app/check_workflows.zig
                 → app/ui/output.zig

app/check_workflows.zig → core/{scanner,github,log}.zig
                         → app/ui/output.zig

app/apply_updates.zig → core/{rewrite,github,log}.zig
                       → app/ui/output.zig

core/scanner.zig → core/log.zig
                 → syntax/github_actions.zig

core/github.zig → core/{git,log}.zig
                → syntax/github_actions.zig

core/rewrite.zig → core/github.zig
                 → syntax/github_actions.zig (tests only)

core/log.zig → app/ui/styles.zig  ← layer violation: core imports app

syntax/github_actions.zig → syntax/yaml_tree.zig

syntax/yaml_tree.zig → tree-sitter (no internal deps)
```

### Key Types

- **`github.Candidate`** — The central type. Contains: `action`, `job`, `current` (tag/SHA), `current_ref` (resolved SHA), `version_comment`, `sha_mismatch`, `next`, `next_label`, `next_is_major`, `file`, `line`, `ref_start`, `ref_end`. Lives in `core/github.zig`.
- **`actions.Reference`** — Parsed from YAML: `kind`, `name` (`ActionName`), `current_ref`, `version_hint`, `scope`, `source` (`SourceLocation`).
- **`CommandInput`** — Parsed CLI flags: `command`, `paths`, `excludes`, `recursive`, `include_branches`, `mode`, `style`, `json`, `dry_run`, `yes`, `verbose`, `ci`. Lives in `cli.zig`.
- **`github.UpdateMode`** / **`github.PinStyle`** — Enums for `major|minor|patch` and `sha|preserve`. In `core/github.zig`.
- **`yaml_tree.Document`** — Parsed tree-sitter YAML document. `deinit()` must be called.
- **`rewrite.TextEdit`** — `{ start, end, replacement }` for in-place file editing.

### Control Flow (update subcommand)

```
1. main.zig → cli.build() → zli dispatches to cmd/update.zig::run()
2. cli.parseCommandInput() parses flags → CommandInput
3. check_workflows.runForCommand():
   a. scanner.scan() → walks directories, finds .yml/.yaml, calls collectReferences()
   b. GitHub Client.resolve() → fetches tags, resolves versions, builds Candidates
4. If --json → output.writeJson(); if --dry-run → output.writePreview()
5. Else: prompt.selectUpdates() → interactive TUI selection → []usize
6. apply_updates.runForCommand() → rewrite.rewriteSelectedFiles() → writes files
```

## Conventions

- **Error handling**: Functions return error unions. `try`/`catch` with targeted error dispatch. `CommandFailed` signals non-zero exit.
- **Memory**: Arena allocators in command handlers. References and Candidates are allocated and must be freed. `deinitReferences`/`deinitCandidates` helpers exist.
- **Tests**: Embedded `test` blocks in each module. Aggregated by `src/tests.zig`. Run with `zig build test`.
- **All imports in Zig files always use `@import` with root-relative paths, and are imported once at the top of files**
- **Do not add Comments to any code unless asked**
- **DO NOT run `zig build test`. It takes forever to compile.**
- **DO NOT use `zig fmt` for formatting**
- **Ansi escape sequences for styles are in `src/app/ui/styles.zig`**

## Known Issues

1. **Circular dependency**: `cli.zig` ↔ `cmd/update.zig` and `cli.zig` ↔ `cmd/validate.zig`. No compile error, but tight coupling.
2. **Layer violation**: `core/log.zig` imports `app/ui/styles.zig` — core depends on the app layer for ANSI constants.
3. **`Candidate` is in `core/github.zig`** — forces UI modules to import the entire GitHub client module for one struct. Candidate also has UI-adjacent methods (`displayTarget`, `shouldWriteVersionComment`).
4. **`github.zig` is overloaded** (400 lines): serves as type library, API client, HTTP handler, tag resolver, and candidate builder.
5. **`output.zig` has domain logic**: `hasShaMismatches()`, `shaMismatchCount()` iterate candidates — presentation module contains query logic.
6. **No integration tests**: All 22 tests are unit-level. Scanner, GitHub client, and full pipeline are untested.
7. **Error handling alias**: `output.writeCheckError = output.writeResolveError` — confusing alias.
8. **`rewrite.zig` imports `syntax/github_actions.zig`** for test-only use of `collectReferences`.
