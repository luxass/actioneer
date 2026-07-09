# Scan pipeline — living specification

> **Version:** v0.1 (2026-06-27)
> Shared analysis layer for `audit` and `update` commands.

## Overview

**Scan once, render many.** Both commands call `scan_workspace()` and receive the same
[`ScanReport`](../src/scan/types.rs). Audit surfaces `issues`; update surfaces `planned`.

```
repo root
    │
    ▼
resolve_workflow_paths()      → .github/workflows/* or explicit PATH args
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

## Pin semantics

**Same SHA ≠ same pin.** `@v4`, `@v4.2.0`, and `@abc123… # v4.2.0` can resolve to the same
commit but represent different pinning intent. Audit and plan share helpers in `src/scan/pin.rs`:

- `classify_tag` / `version_baseline` — major-line (`v4`) vs full semver (`v4.2.0`) from the written pin
- `build_target_value` / `would_change` — skip planning when the on-disk line would not change

### Reusable workflow policy

Local reusable workflow calls such as `./.github/workflows/build.yml` are retained in the scan
report as secondary inventory rows with no audit finding. The scan does not fetch releases,
resolve a GitHub ref, require a SHA pin, or propose an update for them.

Remote reusable workflow calls such as `owner/repo/.github/workflows/build.yml@v1` remain primary
audit targets. They receive GitHub resolution and pin-quality checks, but automatic reusable
workflow updates remain deferred.

### GitHub API budget (per unique remote owner/repo)

| Call | When |
|------|------|
| `list_releases` | Once per repo (cached) |
| `resolve_ref` on current pin | Once per unique `uses:` ref (cached) |
| `resolve_ref` on comment tag | Once for SHA pins with semver comments |
| `resolve_ref` on plan target | Once when an update is proposed |
| `resolve_ref` during major-line infer | Only when `@v4` needs effective semver; newest releases first, stops on match |

Release tag names supply semver for candidate selection — **no bulk resolve of every release tag**.

Major-line tags (`@v4`) are never mapped to invented semver (e.g. `4.0.0`). When needed, the planner
walks release tags newest-first until the resolved SHA matches, and may still plan `@v4` → `@v4.2.0`
when both resolve to the same commit (pin normalization).

Unreleased SHAs (not matching any official release tag) fail audit with `UnreleasedCommit` but do
not block updates — the planner remediates toward the latest official release.

## Module layout

| Path | Role |
|------|------|
| `src/discovery.rs` | Locate workflow files (default or explicit PATH) |
| `src/scan/mod.rs` | `scan_workspace` orchestration |
| `src/scan/types.rs` | `ScanReport`, `ReferenceReport`, `AuditIssue`, `PlannedChange` |
| `src/scan/pin.rs` | Shared pin classification and target-line construction |
| `src/scan/audit.rs` | Audit rules (pure) |
| `src/scan/plan.rs` | Update planner (pure + GitHub resolve for target SHA) |
| `src/scan/apply.rs` | Write planned changes to workflow files |
| `src/engine/uses_line.rs` | Parse and rebuild `uses:` source lines |

## Public API

```rust
pub fn scan_workspace(
    root: &Path,
    workflow_paths: &[PathBuf],  // empty = .github/workflows/
    config: &ActioneerConfig,
    client: &GitHubClient,
) -> Result<ScanReport, ScanError>
```

### CLI workflow targets

Optional positional `PATH` arguments (after flags/subcommand):

| Invocation | Scans |
|------------|-------|
| `actioneer` | `root/.github/workflows/*.{yml,yaml}` |
| `actioneer testdata/workflows/advanced.yml` | that file |
| `actioneer testdata/workflows` | `*.{yml,yaml}` directly in that directory (flat) |
| `actioneer audit PATH` / `actioneer update PATH` | same semantics |

Paths are relative to cwd (`root`). Apply writes back using the same relative paths.

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
| `CommentMajorLineMismatch` | No |
| `FloatingMajorPin` | No |
| `UnreleasedCommit` | No (audit fails; update remediates) |
| `UpdateBlockedByConfig` | No |
| `ReleaseTooYoung` | **Yes** |
| `SkippedBranch` | **Yes** |
| `SecondaryReference` | No (docker/local-action inventory; local reusable workflows emit no finding) |
| `ResolutionFailed` | **Yes** |

### Update planner

- Only `ReferenceKind::Action` with `is_updatable() == true`
- Uses GitHub **Releases API** (`list_releases`) + semver filtering by `config.update`
- Major-line tags: infer semver from SHA; never look up a tag named `v4` as a candidate
- Skips when `would_change` is false (pin string unchanged after apply)
- Unreleased SHAs: remediation mode (ignores update level when targeting official releases)
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
