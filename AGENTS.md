# AGENTS.md — actioneer

Guide for AI agents working on this repository. User-facing docs: `README.md` and `docs/*.md` (living specs — update when design changes).

## Purpose

**actioneer** is a Rust CLI that audits GitHub Actions workflow pins and proposes/applies updates. It discovers `.github/workflows/*.{yml,yaml}`, parses `uses:` references, resolves them via GitHub, flags pin issues, plans semver-bump updates, and optionally rewrites workflow files.

## Branch context

| Branch | State |
|--------|-------|
| `rewrite/from-spec` | **Active rewrite.** Clean layered architecture; scan-once pipeline; TUI default for update. Reset from minimal baseline then rebuilt (config → cache → engine → github → scan → apply → TUI). |
| `main` | Pre-rewrite codebase. Has legacy features (e.g. `--fix`) not ported to rewrite. Do not assume parity. |

Target merge: rewrite replaces main when feature-complete and README/docs updated.

## Architecture map

```
src/
├── main.rs          CLI entry; routes subcommands; TUI vs plain/json for update
├── cli.rs           clap: Audit | Update | Version + global ConfigArgs
├── config.rs        ActioneerConfig; load .github/actioneer.toml; CLI overrides
├── cache.rs         CacheDir resolution (ACTIONEER_CACHE / XDG / ~/.cache)
├── discovery.rs     resolve_workflow_paths(root, targets) → sorted paths relative to root
├── engine/          Pure YAML parse; ActionReference, PinKind, ReferenceKind; no I/O
│   ├── parse.rs     parse_workflow, line numbers, comment extraction
│   ├── reference.rs parse_uses, classify_ref
│   └── uses_line.rs split/join uses: lines for apply
├── github/          GitHubClient: resolve_ref, list_releases; disk cache
├── scan/            Shared pipeline for audit + update (scan once, render many)
│   ├── mod.rs       scan_workspace orchestration
│   ├── types.rs     ScanReport, ReferenceReport, AuditIssue, PlannedChange
│   ├── audit.rs     Pure audit rules
│   ├── plan.rs      Update planner (releases + semver + config)
│   ├── apply.rs     Rewrite uses: lines on disk
│   └── display.rs   Label helpers for plain/TUI output
├── cmd/             Command handlers (audit, update, version)
├── tui/             ratatui + crossterm interactive update UI
└── ansi.rs          Semantic colors for post-TUI apply summary

tests/               Integration tests (engine, github, scan, apply, config, cache, discovery)
testdata/            Workflow YAML + GitHub API JSON fixtures
docs/                Living specs: engine.md, github.md, scan.md, tui.md
```

**Data flow:** `resolve_workflow_paths` → `parse_workflow` → `GitHubClient::resolve_ref` / `list_releases` → `audit::evaluate` + `plan::propose` → `ScanReport` → command render or `apply`.

**CLI paths:** optional positional `PATH` (file or flat directory); default is `.github/workflows/`. Example: `actioneer testdata/workflows/advanced.yml` opens update TUI for that file.

## Commands

| Invocation | Behavior |
|------------|----------|
| `actioneer` / `actioneer update` | TUI by default (unless `--mode plain` or `--mode json`) |
| `actioneer audit` | Scan + print issues; exit 1 if any issues |
| `actioneer update --mode plain` | Print planned updates table |
| `actioneer update --mode json` | JSON array of planned changes |
| `actioneer update --apply` | Apply all planned updates |
| `actioneer update --dry-run` | Preview apply (implies apply; no writes) |
| `actioneer version` | Print version |

Config file: `.github/actioneer.toml` (optional; defaults apply). CLI flags override config (`cli::ConfigArgs` → `ActioneerConfig::apply_overrides`).

**Pin modes:** `sha` (default) writes `@<full-sha> # <tag>`; `tag` writes `@<tag>`.

**Blocking audit issues** (no update planned): `ReleaseTooYoung`, `SkippedBranch`, `ResolutionFailed`.

## Implemented vs deferred

### Implemented (v0.1 rewrite)

- Workflow discovery + engine parse (actions, reusable workflows, docker, local)
- GitHub ref resolution + release listing + disk cache
- Audit: mutable branch, short SHA, not SHA-pinned, comment mismatch, min-release-age, secondary refs, resolution failures
- Update planner: semver bump (major/minor/patch), skip-branches, pin sha/tag
- Apply with on-disk line verification; CLI and TUI paths
- TUI: background scan, selection, apply-on-Enter, exit with colored summary
- Offline / no-cache modes

### Deferred (do not implement without spec update)

- Reusable-workflow updates (audit only today)
- Composite `action.yml` parsing
- Async parallel GitHub fetches
- Cache TTL enforcement / `cache clear` command
- Rate-limit retry/backoff
- Non-`v`-prefixed tag heuristic
- TUI: scrollbar, `?` help, mouse
- Matrix job expansion

See `docs/scan.md` § Deferred and `docs/engine.md` § Open questions.

## Conventions

1. **Scan once, render many** — `scan_workspace()` produces `ScanReport`; audit/update/TUI only render or apply it.
2. **Engine is pure** — no I/O, network, or discovery in `engine/`.
3. **Filter at scan layer** — engine parses all reference kinds; audit/update decide tier via `AuditTier` / `is_updatable()`.
4. **No section-divider comment blocks** — avoid `// =====` or `// --- Section ---` style headers in new code.
5. **Tests** — integration tests in `tests/`; small unit tests allowed in modules. Fixtures in `testdata/`.
6. **Living specs** — when changing design, update the relevant `docs/*.md` in the same PR.
7. **Config validation** — `offline` + `no_cache` is rejected (`ConfigError::Conflict`).
8. **Determinism** — discovery returns lexicographically sorted paths.

## Build and test

```sh
cargo build
cargo test --locked   # preferred
just ci               # fmt-check + clippy + rustdoc/doctests + test
just test / just lint / just check
```

CI (`.github/workflows/ci.yaml`): build + test on ubuntu/macos/windows. Rust edition 2024, MSRV 1.95.

Live GitHub tests are `#[ignore]`; default suite uses `testdata/github/` fixtures.

## TUI UX decisions (do not regress)

- **Default for update** — TUI unless `--mode plain` or `--mode json`.
- **No confirm dialog** — Enter applies immediately.
- **Exit on apply** — TUI closes after apply; summary printed to stdout via `cmd::update::print_apply_plain`.
- **Rows default unselected** — user toggles with Space; `a` select all, `n` deselect all.
- **Semantic colors** — `theme.rs` roles (workflow=cyan, action=amber, from=muted, to=green); post-TUI uses `ansi.rs` when stdout is a TTY.
- **Background scan** — spinner while `scan_workspace` runs on worker thread.
- **Panic safety** — panic hook restores terminal (raw mode + alternate screen).

## Key files for common tasks

| Task | Start here |
|------|------------|
| New audit rule | `src/scan/audit.rs`, `src/scan/types.rs`, `docs/scan.md` |
| Update planning logic | `src/scan/plan.rs` |
| File rewriting | `src/scan/apply.rs`, `src/engine/uses_line.rs` |
| Parse new uses: form | `src/engine/reference.rs`, `docs/engine.md` |
| GitHub API / cache | `src/github/mod.rs`, `src/github/cache.rs` |
| CLI flags | `src/cli.rs`, `src/config.rs` |
| TUI behavior | `src/tui/app.rs`, `docs/tui.md` |

## Pitfalls

- Same `uses:` value repeated in a file can confuse line-number assignment (documented in `docs/engine.md`).
- `serde_yaml 0.9` is deprecated upstream but still used; migration path noted in engine spec.
