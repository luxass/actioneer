# Scan pipeline — living specification

> **Version:** v0.1 (2026-06-27)
> Shared analysis layer for `audit` and `update` commands.

## Overview

**Scan once, render many.** Both commands call `scan_workspace()` and receive the same
[`ScanReport`](src/scan/types.rs). Audit surfaces `issues`; update surfaces `planned`.

```
repo root
    │
    ▼
discover_workflows()          → .github/workflows/*.{yml,yaml}
    │
    ▼
parse_workflow()              → ActionReference (engine)
    │
    ▼
GitHubClient::resolve_ref()   → ResolvedRef (current pin)
GitHubClient::list_releases()   → candidate versions (update planner)
    │
    ▼
audit::evaluate()             → Vec<AuditIssue>
plan::propose()               → Option<PlannedChange>
apply()                       → ApplyReport (when requested)
    │
    ▼
ScanReport
    ├─► actioneer audit  (plain / json)
    └─► actioneer update (plain / json / tui)
```

## Module layout

| Path | Role |
|------|------|
| `src/discovery.rs` | Locate workflow files |
| `src/scan/mod.rs` | `scan_workspace` orchestration |
| `src/scan/types.rs` | `ScanReport`, `ReferenceReport`, `AuditIssue`, `PlannedChange` |
| `src/scan/audit.rs` | Audit rules (pure) |
| `src/scan/plan.rs` | Update planner (pure + GitHub resolve for target SHA) |
| `src/scan/apply.rs` | Write planned changes to workflow files |
| `src/engine/uses_line.rs` | Parse and rebuild `uses:` source lines |

## Public API

```rust
pub fn scan_workspace(
    root: &Path,
    config: &ActioneerConfig,
    client: &GitHubClient,
) -> Result<ScanReport, ScanError>
```

## Shared types

### `ReferenceReport`

One row per `uses:` reference:

| Field | Purpose |
|-------|---------|
| `resolved` | `LocatedReference` + `ResolvedRef` + `CommentMatch` |
| `issues` | Audit findings |
| `planned` | Proposed update, if any |

### Audit issues (v1)

| Issue | Blocks update? |
|-------|----------------|
| `MutableBranch` | No |
| `ShortSha` | No |
| `NotShaPinned` | No |
| `CommentMismatch` | No |
| `ReleaseTooYoung` | **Yes** |
| `SkippedBranch` | **Yes** |
| `SecondaryReference` | No (docker/local inventory) |
| `ResolutionFailed` | **Yes** |

### Update planner

- Only `ReferenceKind::Action` with `is_updatable() == true`
- Uses GitHub **Releases API** (`list_releases`) + semver filtering by `config.update`
- Respects `min_release_age`, `skip_branches`, `pin` (sha vs tag output)

### Apply

- `scan::apply(root, report, targets, config, dry_run)` rewrites `uses:` lines in place
- Verifies the on-disk line still matches what was scanned before writing
- SHA mode writes `@<sha> # <version>`; tag mode writes `@<tag>` (updates comment only if one existed)
- CLI: `--apply` writes all planned rows; `--dry-run` previews without writing
- TUI: Enter applies selected rows and exits; summary printed to stdout

## Command wiring

| Command | Input | Output |
|---------|-------|--------|
| `audit` | `ScanReport` | Issues grouped by workflow (plain/json) |
| `update` | `ScanReport` | Planned changes (plain/json/tui) or apply report (`--apply`) |

Exit code: `audit` returns non-zero when `stats.issues > 0`.

## Deferred

- Reusable-workflow updates
- Async parallel GitHub fetches
- Release list cache TTL enforcement
