# Engine - living specification

> **Version:** v0.1 (2026-06-27)
> This document tracks design decisions, assumptions, and open questions for the
> `actioneer::engine` module. Update it when the design changes.

## Overview

The engine is a pure parsing layer: YAML content in → typed references out.
It does **no** I/O, no network requests, and no file discovery.

```
&str (raw workflow YAML)
        │
        ▼
  parse_workflow()
        │
        ├─ serde_yaml → RawWorkflow (internal)
        │
        ├─ for each jobs.<id>:
        │    • job-level `uses` (reusable workflow call)
        │    • step-level `uses` (per step)
        │
        ├─ parse_uses() per raw string
        │
        └─ assign_line_numbers() (forward scan)
                │
                ▼
         WorkflowDocument { name, references: Vec<ActionReference> }
```

## Module layout

| Path | Role |
|------|------|
| `src/engine/mod.rs` | Public types: `ActionReference`, `AuditTier`, `PinKind`, `ReferenceKind`, `WorkflowDocument`, `ParseError`, `CommentMatch`; public functions: `parse_workflow`, `comment_matches_ref` |
| `src/engine/parse.rs` | `parse_workflow` + `assign_line_numbers` + `extract_uses_comment`; internal `RawWorkflow`/`RawJob`/`RawStep` serde structs |
| `src/engine/reference.rs` | `parse_uses` + `classify_ref` |
| `testdata/workflows/` | YAML fixtures for integration tests |
| `tests/engine.rs` | Integration test suite |

## Public API

```rust
pub fn parse_workflow(content: &str) -> Result<WorkflowDocument, ParseError>
pub fn comment_matches_ref(reference: &ActionReference) -> CommentMatch
```

`parse_workflow` is the primary entry point. `comment_matches_ref` operates on an already-parsed
`ActionReference` and is a pure utility with no I/O.

## Types

### `ReferenceKind`

Classifies **what** is referenced (orthogonal to how it is pinned):

| Variant | Example `uses:` value |
|---------|----------------------|
| `Action` | `actions/checkout@v4` |
| `LocalAction` | `./my-local-action` |
| `Docker` | `docker://alpine:3.14` |
| `ReusableWorkflow` | `./.github/workflows/ci.yml` or `org/repo/.github/workflows/ci.yml@v1` |

Methods on `ReferenceKind`:

| Method | Return type | Purpose |
|--------|------------|---------|
| `audit_tier()` | `AuditTier` | Which audit tier applies |
| `is_updatable()` | `bool` | Whether automatic updates are supported |

### `AuditTier`

Classifies how a `ReferenceKind` participates in audit and update operations.
The engine always parses every reference regardless of tier; filtering belongs in the
audit/update layer.

| Variant | Meaning |
|---------|---------|
| `Primary` | Full pin checks; automatic updates supported (where implemented) |
| `Secondary` | Partial checks / warnings only; no automatic updates in the current iteration |

Tier assignment per `ReferenceKind`:

| `ReferenceKind` | `AuditTier` | `is_updatable()` | Notes |
|-----------------|-------------|-----------------|-------|
| `Action` | `Primary` | `true` | Full pin checks and SHA update |
| `ReusableWorkflow` | `Primary` | `false` | Ref checks; update deferred |
| `Docker` | `Secondary` | `false` | Tag/digest warnings only |
| `LocalAction` | `Secondary` | `false` | Inventory only |

### `PinKind`

Classifies **how** the `@ref` component is pinned:

| Variant | Rule |
|---------|------|
| `FullSha` | Exactly 40 all-hex characters |
| `ShortSha` | 7–39 all-hex characters |
| `Tag` | Starts with `v` followed by an ASCII digit |
| `Branch` | Anything else (`main`, `master`, `feature/foo`, `HEAD`, ...) |
| `Unpinned` | No `@ref` at all (local actions, Docker, bare `owner/repo`) |

### `ActionReference` fields

| Field | Type | Notes |
|-------|------|-------|
| `raw` | `String` | Exact value from `uses:` |
| `kind` | `ReferenceKind` | What kind of entity |
| `pin_kind` | `PinKind` | How the ref is pinned |
| `owner` | `Option<String>` | GitHub user/org; `None` for local/docker |
| `repo` | `Option<String>` | Repository name; `None` for local/docker |
| `subpath` | `Option<String>` | Third segment+; local path; docker image:tag |
| `git_ref` | `Option<String>` | The `@ref` part only |
| `step_name` | `Option<String>` | Step `name:` field if present |
| `job_id` | `String` | Map key under `jobs:` |
| `job_name` | `Option<String>` | Job `name:` field if present |
| `step_index` | `Option<usize>` | Zero-based index; `None` for job-level `uses:` |
| `line` | `Option<u32>` | 1-based line number (best-effort) |
| `line_comment` | `Option<String>` | Trailing comment text (without `#`); `None` if absent or empty |

### `CommentMatch`

Returned by [`comment_matches_ref`] after comparing `line_comment` against `git_ref`:

| Variant | Meaning |
|---------|---------|
| `NoComment` | No trailing comment on the `uses:` line |
| `Match` | Comment corresponds to the pinned ref |
| `Mismatch { comment, expected }` | Comment is present but does not match |

## Assumptions and design decisions

### `uses:` forms supported

1. `owner/repo@ref` - standard action
2. `owner/repo/sub/path@ref` - nested path action
3. `./path` or `../path` - local action (no `@ref`)
4. `docker://image:tag` - Docker image
5. `./.github/workflows/foo.yml[@ref]` - local reusable workflow (job-level or step-level)
6. `owner/repo/.github/workflows/foo.yml@ref` - remote reusable workflow

Reusable-workflow detection heuristic: path segment contains `.github/workflows/` AND
ends with `.yml` or `.yaml`. This is unambiguous in practice because regular action
sub-paths do not follow that naming convention.

### Job-level `uses:` (reusable workflows)

GitHub Actions permits `jobs.<id>.uses` for calling a reusable workflow.
In this case the job has no `steps:` array, and the reference gets
`step_index: None` and `step_name: None`.

Reference: [GitHub docs - reusing workflows](https://docs.github.com/en/actions/using-workflows/reusing-workflows)

**Assumption:** Job-level `uses` always refers to a reusable workflow, never a regular
action. The GHA schema enforces this.

### Inline comment extraction (Renovate-style pinning)

`serde_yaml` strips YAML comments before deserialization, so the trailing comment on a
`uses:` line is not available through normal deserialization. The forward scanner in
`assign_line_numbers` re-reads the raw source to extract it.

**Extraction rules:**

- Comment is everything after the first `#` on the same line as the `uses:` key.
- Leading/trailing whitespace in the comment is stripped.
- A bare `#` with no text after it → `line_comment: None`.
- A `#` inside a quoted `uses:` value is NOT treated as a comment delimiter (e.g.
  `uses: "owner/repo@ref"` - the closing quote is found first, then `#` is searched
  in the remainder).
- Stored in `ActionReference::line_comment`.

**Matching via `comment_matches_ref`:**

| Input | Result |
|-------|--------|
| No `line_comment` | `NoComment` |
| `line_comment == git_ref` (exact) | `Match` |
| `pin_kind == FullSha` and `line_comment` contains `git_ref` | `Match` |
| Anything else | `Mismatch { comment, expected }` |

The `line` + `line_comment` fields together provide everything the future patching layer
needs to locate and rewrite the comment without a full YAML re-parse.

### Line tracking

After YAML deserialization, a forward scan of the raw content matches each reference
to the next `uses:` line in document order. The scanner handles both:
- `        uses: value` (key on its own line inside a mapping block)
- `      - uses: value` (key as the first key on an inline sequence entry)

**Limitation:** If the same `uses:` value appears multiple times in a job and references
happen to be extracted out of source order (e.g., due to future async processing), line
numbers may be off by one occurrence. For v0.1 this is acceptable; a proper solution
would use a YAML parser that exposes source marks (e.g., `marked_yaml`).

### YAML library choice

Uses `serde_yaml 0.9.x` (wraps `unsafe-libyaml`, YAML 1.2 subset).

**Known:** `serde_yaml 0.9` is marked deprecated upstream. It remains functional and is
the most widely deployed Rust YAML+serde integration. Migration path when needed:
- `serde_yaml_ng` (community continuation of 0.9) - drop-in swap
- `marked_yaml` - adds source location support at the cost of a different deserialization model

The `on:` top-level trigger key is ignored during deserialization (only `name` and `jobs`
are extracted), so no conflict with YAML 1.1 boolean parsing of `on`.

### IndexMap for job order

`jobs:` is deserialized into `IndexMap<String, RawJob>` (from the `indexmap` crate) to
preserve document order. This is required for the line-number scan to work correctly.
`HashMap` would give arbitrary iteration order.

### SHA threshold

- **Full SHA:** exactly 40 hex characters.
- **Short SHA:** 7–39 hex characters (all hex digits, case-insensitive).
  The lower bound of 7 matches git's default `--abbrev` length.

This is a heuristic. A 7-character all-hex string could also be a branch name (unlikely
but possible). See Open Questions below.

### Tag detection

A `@ref` is classified as `Tag` if it starts with `v` followed immediately by an ASCII
digit (`v4`, `v1.2.3`, `v2.0.0-rc1`). This covers all standard GHA action tags.
Non-`v`-prefixed tags (e.g., `2.0`, `release-2024`) are currently classified as `Branch`.
This is intentional and documented as a future refinement.

## Open questions

These questions need owner decisions before the affected feature can be built.
Update this section as decisions are made.

### OQ-1: Composite `action.yml` files

Should the engine also parse `action.yml` / `action.yaml` files to extract `uses:` entries
from composite action steps?

**Impact:** Would require a separate entry point (e.g., `parse_action`) and possibly a
separate type alongside `WorkflowDocument`.

**Decision (2026-06-27):** Out of scope for now. Workflows only. Revisit when file
discovery or composite-action audit is implemented.

### OQ-2: Reusable workflows vs. regular actions - same type or separate?

Currently both use `ActionReference`, distinguished by `kind: ReferenceKind::ReusableWorkflow`.
Should reusable workflow references have their own type with extra fields (e.g., `inputs`,
`secrets`)?

**Decision (2026-06-27):** Keep unified `ActionReference` + `ReferenceKind`. Split only if
audit/update logic needs dedicated fields (inputs, secrets).

### OQ-3: Short SHA detection threshold

The current threshold (7–39 hex chars) means a 7-character all-hex branch name like
`abc1234` would be misclassified as `ShortSha`. Should the minimum be raised (e.g. 12)?

**Tradeoff:** Higher threshold → fewer false positives, but misses very short SHAs.
In practice most short SHAs in GHA pinning are ≥ 7 chars.

**Decision (2026-06-27):** Keep 7-char minimum (matches git default `--abbrev`).

### OQ-4: Matrix-expanded steps

Steps inside matrix jobs produce identical `uses:` values but may refer to different
effective environments. For v0.1 we parse them identically (no matrix expansion).

**Decision needed for:** audit/update features that need to understand matrix context.

### OQ-5: Non-`v`-prefixed tags

Tags like `2.0`, `latest`, or `release-2024` are classified as `Branch` today.
Should `PinKind` get a `OtherTag` variant, or should the heuristic be broadened?

**Decision (2026-06-27):** Defer until a real action using this style blocks audit/update.

### OQ-7: Renovate-style trailing comments on `uses:` lines

Tools like Renovate pin actions to a SHA and write the human-readable tag in a trailing
comment: `uses: actions/checkout@deadbeef # v4.2.0`. Should the engine parse and expose
this comment so the audit layer can verify or update it?

**Decision (2026-06-27):** Yes - implemented.

- `ActionReference::line_comment: Option<String>` stores the comment text (without `#`).
- `comment_matches_ref(&ActionReference) -> CommentMatch` compares it against `git_ref`.
- The raw comment text plus `line` is sufficient for the future update layer to patch
  the comment in-place; actual file rewriting is deferred.

### OQ-6: Column / byte offset

`line` is a 1-based line number. A `column` field (1-based) or `byte_offset` (from the
start of the file) would be needed for zero-diff patching. The current forward scan can
provide column cheaply. Defer until the patching layer is designed.

### OQ-8: Docker and local-action scope in audit/update

Should `Docker` and `LocalAction` references participate in audit and update commands,
or should they be silently skipped?

**Context:** The engine parses all four `ReferenceKind` variants unconditionally.
Higher-level commands must decide what to do with each.

**Decision (2026-06-27):** Parse always; filter at the audit/update layer, not in the
engine. Tier table:

| `ReferenceKind` | Audit tier | Update |
|-----------------|-----------|--------|
| `Action` | Primary — full pin checks | Yes |
| `ReusableWorkflow` | Primary — ref checks | Deferred |
| `Docker` | Secondary — tag/digest warnings | No |
| `LocalAction` | Secondary — inventory only | No |

`AuditTier` enum and `ReferenceKind::audit_tier()` / `ReferenceKind::is_updatable()`
are type-level hooks for this classification. No config flags (`include_local`,
`include_docker`) are needed in this iteration.

## Future hooks

These integration points are intentionally left unimplemented:

- **File discovery** - locating `*.yml` files under `.github/workflows/` belongs in a
  higher-level layer, not the engine.
- **GitHub API / version resolution** - no network calls from the engine, ever.
- **Caching** - the engine is stateless; caching wraps it from the outside.
- **Patching** - textual substitution will use `line`/`column` from `ActionReference`.
- **`action.yml` composite actions** - see OQ-1.
