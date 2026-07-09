# GitHub Client - living specification

> **Version:** v0.1 (2026-06-27)
> This document tracks design decisions, assumptions, and open questions for the
> `actioneer::github` module. Update it when the design changes.

## Overview

The GitHub client resolves `owner/repo@ref` strings (produced by the engine
layer) to commit SHAs and lists repository releases. The scan layer matches a
resolved tag to that releases index to enforce `min-release-age` and plan
updates without a per-reference release request.

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
         ├─ Write to cache (unless no_cache)
         │
         └── ResolvedRef { sha, ref_kind, published_at }

  list_releases(owner, repo)
         ├─ Read releases/index.json (unless no_cache)
         ├─ GET /repos/{owner}/{repo}/releases on cache miss
         └── scan layer enriches matching ResolvedRef values
```

## Module layout

| Path | Role |
|------|------|
| `src/github/mod.rs` | Public types: `GitHubClient`, `ResolvedRef`, `RefKind`, `GitHubError`; private: response types, HTTP helpers, resolution logic |
| `src/github/cache.rs` | `CacheEntry`; ref/release-index paths, cache reads/writes, `now_secs` |
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
GitHubClient::list_releases(&self, owner: &str, repo: &str)
    -> Result<Vec<Release>, GitHubError>

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
      index.json
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

### List repository releases

```
GET /repos/{owner}/{repo}/releases?per_page=100&page={page}
```

The client follows up to ten pages, ignores releases without `published_at`, and
caches the result at `releases/index.json`. The scan layer enriches a resolved
tag when its name matches an entry. `resolve_ref` does not make a separate
`releases/tags/{tag}` request.

## Request headers

| Header | Value |
|--------|-------|
| `Accept` | `application/vnd.github+json` |
| `User-Agent` | `actioneer/<version>` |
| `X-GitHub-Api-Version` | `2022-11-28` |
| `Authorization` | `Bearer <token>` when a token is resolved (see Authentication) |

## Authentication

Tokens are resolved once when [`GitHubClient::new`] is called:

| Priority | Source |
|----------|--------|
| 1 | `GITHUB_TOKEN` environment variable (**always preferred** when set) |
| 2 | `gh auth token` (GitHub CLI, when installed and logged in) |
| 3 | No token — proceed unauthenticated |

Without a token, requests still go out but are limited to **60 requests/hour**.
Authenticated requests get **5,000 requests/hour**.

Set `GITHUB_TOKEN` explicitly in CI. Locally, `gh auth login` is enough if the
CLI is on your `PATH`.

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

### Release date comes from the releases index

Tag `published_at` is taken from the cached `list_releases` index when the pin
tag matches a release name. `resolve_ref` no longer makes a separate
`releases/tags/{tag}` request per tag lookup.

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

- **File discovery** — the client does not search for workflow files; that
  belongs in a separate layer.
- **Cache TTL** — cache entries record `fetched_at`, but expiration and manual
  clearing remain deferred.
- **Rate-limit retry** — rate-limit errors are returned immediately; retry and
  backoff remain deferred.
