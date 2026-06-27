# GitHub Client - living specification

> **Version:** v0.1 (2026-06-27)
> This document tracks design decisions, assumptions, and open questions for the
> `actioneer::github` module. Update it when the design changes.

## Overview

The GitHub client resolves `owner/repo@ref` strings (produced by the engine
layer) to commit SHAs. It also fetches the GitHub Release publication date for
future `min-release-age` filtering.

```
ActioneerConfig { offline, no_cache, ... }
CacheDir (optional)
         │
         ▼
  GitHubClient::new(config, cache)
         │
         │  resolve_ref(owner, repo, git_ref)
         │
         ├─ Full SHA? ─────────────────────► return immediately (no I/O)
         │
         ├─ Read cache (unless no_cache)
         │    hit? ──────────────────────► return CacheEntry
         │
         ├─ offline + cache miss? ──────► GitHubError::Offline
         │
         ├─ Fetch from GitHub API
         │    GET /repos/{owner}/{repo}/git/ref/tags/{tag}
         │    or  /repos/{owner}/{repo}/git/ref/heads/{branch}
         │    (annotated tag? dereference via /git/tags/{sha})
         │
         ├─ Fetch release date (best-effort, tags only)
         │    GET /repos/{owner}/{repo}/releases/tags/{tag}
         │
         ├─ Write to cache (unless no_cache)
         │
         └── ResolvedRef { sha, ref_kind, published_at }
```

## Module layout

| Path | Role |
|------|------|
| `src/github/mod.rs` | Public types: `GitHubClient`, `ResolvedRef`, `RefKind`, `GitHubError`; private: response types, HTTP helpers, resolution logic |
| `src/github/cache.rs` | `CacheEntry`; `ref_path`, `release_path`, `read_entry`, `write_entry`, `now_secs` |
| `testdata/github/` | JSON fixtures matching real GitHub API response shapes |
| `tests/github.rs` | Integration test suite (cache-based; live tests are `#[ignore]`) |

## Public API

```rust
// Construction
GitHubClient::new(config: &ActioneerConfig, cache: Option<CacheDir>) -> GitHubClient
GitHubClient::with_base_url(self, url: impl Into<String>) -> Self  // test helper

// Resolution
GitHubClient::resolve_ref(&self, owner: &str, repo: &str, git_ref: &str)
    -> Result<ResolvedRef, GitHubError>

// Types
pub struct ResolvedRef { pub sha: String, pub ref_kind: RefKind, pub published_at: Option<String> }
pub enum RefKind { Tag, Branch, Sha }
pub enum GitHubError { Offline, Http, RateLimited, NotFound, CacheRead, CacheWrite, Json, Transport }
pub struct CacheEntry { pub sha, pub ref_kind, pub published_at, pub fetched_at }
```

## Cache layout

```
<CacheDir>/github/
  <owner>/<repo>/
    refs/
      tags/<encoded_tag>.json
      heads/<encoded_branch>.json
    releases/
      <encoded_tag>.json
```

Encoding: `/` in ref names is replaced with `%2F`. All other characters used in
valid git ref names are preserved as-is.

### CacheEntry JSON format

```json
{
  "sha": "a81bbbf8298c0fa03ea29cdc473d45769f953675",
  "ref_kind": "tag",
  "published_at": "2023-10-16T17:17:35Z",
  "fetched_at": 1697480255
}
```

| Field | Type | Notes |
|-------|------|-------|
| `sha` | `string` | Full 40-char commit SHA |
| `ref_kind` | `"tag"` \| `"branch"` \| `"sha"` | How the ref was classified when fetched |
| `published_at` | `string \| null` | ISO 8601 from GitHub Releases API |
| `fetched_at` | `u64` | Unix timestamp in seconds (for future TTL) |

Writes are atomic: JSON is written to `<path>.json.tmp`, then renamed to
`<path>` (POSIX rename is atomic within the same filesystem).

## Cache policy

| `offline` | `no_cache` | Behaviour |
|-----------|-----------|-----------|
| `false`   | `false`   | Read cache first; fetch + write on miss |
| `true`    | `false`   | Cache read only; `GitHubError::Offline` on cache miss |
| `false`   | `true`    | Network only; no cache reads or writes |
| `true`    | `true`    | Rejected at config validation (`validate()` returns `Conflict` error) |

## GitHub API endpoints

### Resolve tag to commit SHA

```
GET /repos/{owner}/{repo}/git/ref/tags/{tag}
```

Response:
```json
{
  "object": {
    "sha": "...",
    "type": "commit"   // or "tag" for annotated tags
  }
}
```

For annotated tags (`object.type == "tag"`), a second request dereferences the
tag object to the actual commit SHA:

```
GET /repos/{owner}/{repo}/git/tags/{object.sha}
```

Response:
```json
{ "object": { "sha": "...", "type": "commit" } }
```

### Resolve branch to commit SHA

```
GET /repos/{owner}/{repo}/git/ref/heads/{branch}
```

Same response shape as the tag endpoint. `object.type` is always `"commit"` for
branch refs.

### Fetch release date (best-effort)

```
GET /repos/{owner}/{repo}/releases/tags/{tag}
```

Response (partial):
```json
{ "published_at": "2023-10-16T17:17:35Z" }
```

HTTP 404 means the tag has no corresponding GitHub Release — this is treated as
`published_at: None`, not an error.

## Request headers

| Header | Value |
|--------|-------|
| `Accept` | `application/vnd.github+json` |
| `User-Agent` | `actioneer/<version>` |
| `X-GitHub-Api-Version` | `2022-11-28` |
| `Authorization` | `Bearer <GITHUB_TOKEN>` (only when `GITHUB_TOKEN` is set) |

## Error handling

| Condition | Error variant |
|-----------|---------------|
| HTTP 404 | `NotFound { owner, repo, git_ref }` |
| HTTP 403 or 429 | `RateLimited` |
| Other 4xx/5xx | `Http { status, message: "" }` |
| Offline + cache miss | `Offline` |
| Disk read failure | `CacheRead(io::Error)` |
| Disk write failure | `CacheWrite(io::Error)` |
| JSON parse failure | `Json(serde_json::Error)` |
| Transport / TLS | `Transport(String)` |

## HTTP client

Uses [`ureq`](https://crates.io/crates/ureq) 3.x (blocking, pure-Rust TLS via
rustls). A single `ureq::Agent` is stored in `GitHubClient` and reused across
calls for connection pooling. `Agent` is `Clone` (backed by `Arc`) so
`GitHubClient` can be cheaply cloned as well.

`GITHUB_TOKEN` is read once from the environment at construction time.

## Assumptions and design decisions

### Ref classification heuristic

`resolve_ref` classifies the `git_ref` string using the same rule as
`engine/reference.rs`:
- 40 all-hex chars → `RefKind::Sha` (short-circuited, no I/O)
- Starts with `v` + ASCII digit → `RefKind::Tag`
- Anything else → `RefKind::Branch`

This means non-`v`-prefixed tags (e.g. `2.0`, `release-2024`) are queried as
branches, which will fail with HTTP 404 on the `heads/` endpoint. This is
consistent with the engine's `PinKind` heuristic. See OQ-1 below.

### Annotated vs lightweight tags

Most GitHub Actions repos use lightweight tags. The client handles annotated
tags transparently by checking `object.type` in the first response and making a
second request when needed. The extra request is cached as part of the combined
`CacheEntry` result (keyed on the original `git_ref`), so subsequent calls only
require one cache read.

### Release date is best-effort

Not every tag has a GitHub Release entry. The `api_release_date` call returns
`Ok(None)` on HTTP 404. The overall `resolve_ref` call always succeeds if the
ref itself resolves.

### `fetched_at` for future TTL

Every cache entry stores `fetched_at` as a Unix timestamp (seconds). No TTL
eviction is implemented in v0.1; the field is reserved for a future cache
invalidation mechanism. Stale entries are served indefinitely until manually
cleared or until TTL logic is added.

### No cache directory → network only

When `cache` is `None` (e.g. the home directory could not be determined),
the client behaves as if `no_cache = true`: reads and writes are silently skipped.
Offline mode with `cache: None` will always return `GitHubError::Offline`.

## Open questions

### OQ-1: Non-`v`-prefixed tags

Tags like `2.0`, `latest`, or `release-2024` are classified as `Branch` and
resolved against the `heads/` endpoint. This will 404 for most repos.

**Options:**
1. Try `tags/` if `heads/` returns 404 (two extra requests worst case).
2. Add a `RefKind::OtherTag` variant (requires engine coordination).
3. Accept the limitation; document it.

**Decision (2026-06-27):** Accept for v0.1. Only `v`-prefixed tags are supported.

### OQ-2: Cache TTL

Cached entries are never expired. Stale SHAs could remain in the cache
indefinitely after a tag is force-pushed or deleted.

**Options:**
1. TTL on `fetched_at` (e.g. 24h for branches, never for tags).
2. ETag / `If-None-Match` conditional requests.
3. Manual `actioneer cache clear` command.

**Decision (2026-06-27):** Defer to a follow-up. TTL fields are reserved.

### OQ-3: Pagination for tag/release lookups

`GET /repos/{owner}/{repo}/git/ref/tags/{tag}` is a single-item lookup, not a
list endpoint, so pagination is not relevant for the current implementation.

### OQ-4: `skip_branches` config integration

`ActioneerConfig::skip_branches` is intended to skip branch-pinned refs in
audit/update. The GitHub client does not enforce this — it will resolve any ref
it receives. Filtering belongs in the audit/update command layer.

### OQ-5: Rate limit backoff

Currently `RateLimited` is returned immediately. A retry-with-backoff strategy
(respecting `X-RateLimit-Reset`) would reduce noise for high-volume usage.

**Decision (2026-06-27):** Defer to v0.2. No retry logic in v0.1.

## Future hooks

- **`min_release_age` enforcement** — `published_at` is available but the
  comparison against `ActioneerConfig::min_release_age` is not yet implemented.
  The audit layer will do this comparison using the `ResolvedRef::published_at`
  string.
- **Audit / update commands** — the client is intentionally not wired into
  `cmd/audit.rs` or `cmd/update.rs` yet.
- **File discovery** — the client does not search for workflow files; that
  belongs in a separate layer.
