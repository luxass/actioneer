use actioneer::{
    cache::resolve_cache_dir_with,
    config::ActioneerConfig,
    github::{CacheEntry, GitHubClient, GitHubError, RefKind, ResolvedRef},
};
use tempfile::TempDir;

// --- Helpers ---

/// Create a temp [`TempDir`] and a matching [`GitHubClient`] in offline mode.
///
/// The cache directory is pre-populated by the caller before the client is used.
fn offline_client(dir: &TempDir) -> GitHubClient {
    let config = ActioneerConfig {
        offline: true,
        no_cache: false,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    GitHubClient::new(&config, cache)
}

/// Create a temp [`TempDir`] and a [`GitHubClient`] with no_cache=true.
fn no_cache_client() -> (TempDir, GitHubClient) {
    let dir = TempDir::new().unwrap();
    let config = ActioneerConfig {
        offline: false,
        no_cache: true,
        ..Default::default()
    };
    // Pass a cache dir so we can verify nothing is written.
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    (dir, GitHubClient::new(&config, cache))
}

/// Write a pre-built [`CacheEntry`] at the expected cache path for `owner/repo@ref`.
fn seed_cache(dir: &TempDir, owner: &str, repo: &str, kind: &str, git_ref: &str, entry: &CacheEntry) {
    use actioneer::cache::resolve_cache_dir_with;
    use std::fs;

    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
    let encoded = git_ref.replace('/', "%2F");
    let path = cache
        .path()
        .join("github")
        .join(owner)
        .join(repo)
        .join("refs")
        .join(kind)
        .join(format!("{encoded}.json"));

    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(entry).unwrap()).unwrap();
}

// --- Cache hit ---

#[test]
fn cache_hit_tag_returns_cached_sha() {
    let dir = TempDir::new().unwrap();
    let entry = CacheEntry {
        sha: "a81bbbf8298c0fa03ea29cdc473d45769f953675".into(),
        ref_kind: "tag".into(),
        published_at: Some("2023-10-16T17:17:35Z".into()),
        fetched_at: 1_700_000_000,
    };
    seed_cache(&dir, "actions", "checkout", "tags", "v4", &entry);

    let client = offline_client(&dir);
    let resolved = client.resolve_ref("actions", "checkout", "v4").unwrap();

    assert_eq!(resolved.sha, "a81bbbf8298c0fa03ea29cdc473d45769f953675");
    assert_eq!(resolved.ref_kind, RefKind::Tag);
    assert_eq!(
        resolved.published_at.as_deref(),
        Some("2023-10-16T17:17:35Z")
    );
}

#[test]
fn cache_hit_branch_returns_cached_sha() {
    let dir = TempDir::new().unwrap();
    let entry = CacheEntry {
        sha: "b4ffde65f46336ab88eb53be808477a3936bae11".into(),
        ref_kind: "branch".into(),
        published_at: None,
        fetched_at: 1_700_000_000,
    };
    seed_cache(&dir, "actions", "checkout", "heads", "main", &entry);

    let client = offline_client(&dir);
    let resolved = client.resolve_ref("actions", "checkout", "main").unwrap();

    assert_eq!(resolved.sha, "b4ffde65f46336ab88eb53be808477a3936bae11");
    assert_eq!(resolved.ref_kind, RefKind::Branch);
    assert!(resolved.published_at.is_none());
}

#[test]
fn cache_hit_branch_with_slash_in_name() {
    let dir = TempDir::new().unwrap();
    let entry = CacheEntry {
        sha: "deadbeefdeadbeefdeadbeefdeadbeef12345678".into(),
        ref_kind: "branch".into(),
        published_at: None,
        fetched_at: 1_700_000_000,
    };
    seed_cache(&dir, "myorg", "myrepo", "heads", "feature/cool-thing", &entry);

    let client = offline_client(&dir);
    let resolved = client
        .resolve_ref("myorg", "myrepo", "feature/cool-thing")
        .unwrap();

    assert_eq!(resolved.sha, "deadbeefdeadbeefdeadbeefdeadbeef12345678");
    assert_eq!(resolved.ref_kind, RefKind::Branch);
}

// --- Full SHA passthrough ---

#[test]
fn full_sha_returns_immediately_without_cache_or_network() {
    let dir = TempDir::new().unwrap();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let client = offline_client(&dir);

    let resolved = client.resolve_ref("actions", "checkout", sha).unwrap();

    assert_eq!(resolved.sha, sha);
    assert_eq!(resolved.ref_kind, RefKind::Sha);
    assert!(resolved.published_at.is_none());
}

// --- Offline mode + cache miss ---

#[test]
fn offline_cache_miss_returns_error() {
    let dir = TempDir::new().unwrap();
    let client = offline_client(&dir);

    let err = client
        .resolve_ref("actions", "checkout", "v4")
        .unwrap_err();

    assert!(
        matches!(err, GitHubError::Offline),
        "expected Offline, got: {err}"
    );
}

#[test]
fn offline_error_display() {
    let e = GitHubError::Offline;
    assert_eq!(e.to_string(), "offline mode: no cached response available");
}

// --- no_cache mode ---

#[test]
fn no_cache_mode_does_not_write_to_cache() {
    let (dir, client) = no_cache_client();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";

    // Full SHA passthrough - no network, no cache write.
    let _ = client.resolve_ref("actions", "checkout", sha).unwrap();

    let cache_root = dir.path().join("github");
    assert!(
        !cache_root.exists(),
        "no_cache mode must not write anything to cache"
    );
}

// --- Ref classification ---

#[test]
fn resolve_full_sha_is_ref_kind_sha() {
    let dir = TempDir::new().unwrap();
    let client = offline_client(&dir);
    let sha = "0".repeat(40);
    let r = client.resolve_ref("x", "y", &sha).unwrap();
    assert_eq!(r.ref_kind, RefKind::Sha);
}

// --- ResolvedRef + RefKind types ---

#[test]
fn ref_kind_display() {
    assert_eq!(RefKind::Tag.to_string(), "tag");
    assert_eq!(RefKind::Branch.to_string(), "branch");
    assert_eq!(RefKind::Sha.to_string(), "sha");
}

#[test]
fn resolved_ref_equality() {
    let a = ResolvedRef {
        sha: "a".repeat(40),
        ref_kind: RefKind::Tag,
        published_at: Some("2024-01-01T00:00:00Z".into()),
    };
    let b = a.clone();
    assert_eq!(a, b);
}

// --- CacheEntry helpers ---

#[test]
fn cache_entry_ref_kind_tag() {
    let e = CacheEntry {
        sha: "a".repeat(40),
        ref_kind: "tag".into(),
        published_at: None,
        fetched_at: 0,
    };
    assert_eq!(e.ref_kind(), RefKind::Tag);
}

#[test]
fn cache_entry_ref_kind_branch() {
    let e = CacheEntry {
        sha: "b".repeat(40),
        ref_kind: "branch".into(),
        published_at: None,
        fetched_at: 0,
    };
    assert_eq!(e.ref_kind(), RefKind::Branch);
}

#[test]
fn cache_entry_ref_kind_sha() {
    let e = CacheEntry {
        sha: "c".repeat(40),
        ref_kind: "sha".into(),
        published_at: None,
        fetched_at: 0,
    };
    assert_eq!(e.ref_kind(), RefKind::Sha);
}

// --- GitHubError display ---

#[test]
fn error_not_found_display() {
    let e = GitHubError::NotFound {
        owner: "actions".into(),
        repo: "checkout".into(),
        git_ref: "v99".into(),
    };
    assert_eq!(e.to_string(), "actions/checkout@v99: ref not found on GitHub");
}

#[test]
fn error_rate_limited_display() {
    assert_eq!(
        GitHubError::RateLimited.to_string(),
        "GitHub API rate limit exceeded"
    );
}

#[test]
fn error_http_with_message_display() {
    let e = GitHubError::Http {
        status: 500,
        message: "internal server error".into(),
    };
    assert_eq!(
        e.to_string(),
        "GitHub API error (HTTP 500): internal server error"
    );
}

#[test]
fn error_http_without_message_display() {
    let e = GitHubError::Http {
        status: 503,
        message: String::new(),
    };
    assert_eq!(e.to_string(), "GitHub API error (HTTP 503)");
}

// --- Fixture JSON deserialization ---
//
// These tests verify that our internal serde types (GitRefResponse, etc.) match
// the shape of the real GitHub API responses stored in testdata/github/.

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/testdata/github/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

#[test]
fn fixture_git_ref_tag_lightweight_sha() {
    #[derive(serde::Deserialize)]
    struct GitRefResponse {
        object: GitRefObject,
    }
    #[derive(serde::Deserialize)]
    struct GitRefObject {
        sha: String,
        #[serde(rename = "type")]
        kind: String,
    }

    let resp: GitRefResponse = serde_json::from_str(&fixture("git_ref_tag_lightweight.json")).unwrap();
    assert_eq!(resp.object.sha, "a81bbbf8298c0fa03ea29cdc473d45769f953675");
    assert_eq!(resp.object.kind, "commit");
}

#[test]
fn fixture_git_ref_tag_annotated_type() {
    #[derive(serde::Deserialize)]
    struct GitRefResponse {
        object: GitRefObject,
    }
    #[derive(serde::Deserialize)]
    struct GitRefObject {
        #[serde(rename = "type")]
        kind: String,
    }

    let resp: GitRefResponse = serde_json::from_str(&fixture("git_ref_tag_annotated.json")).unwrap();
    assert_eq!(resp.object.kind, "tag", "annotated tag object type must be 'tag'");
}

#[test]
fn fixture_git_tag_annotated_deref_commit_sha() {
    #[derive(serde::Deserialize)]
    struct GitTagResponse {
        object: GitTagInner,
    }
    #[derive(serde::Deserialize)]
    struct GitTagInner {
        sha: String,
    }

    let resp: GitTagResponse =
        serde_json::from_str(&fixture("git_tag_annotated_deref.json")).unwrap();
    assert_eq!(
        resp.object.sha,
        "deadbeefdeadbeefdeadbeefdeadbeef12345678"
    );
}

#[test]
fn fixture_git_ref_branch_main_sha() {
    #[derive(serde::Deserialize)]
    struct GitRefResponse {
        object: GitRefObject,
    }
    #[derive(serde::Deserialize)]
    struct GitRefObject {
        sha: String,
    }

    let resp: GitRefResponse = serde_json::from_str(&fixture("git_ref_branch_main.json")).unwrap();
    assert_eq!(resp.object.sha, "b4ffde65f46336ab88eb53be808477a3936bae11");
}

#[test]
fn fixture_release_published_at() {
    #[derive(serde::Deserialize)]
    struct ReleaseResponse {
        published_at: Option<String>,
    }

    let resp: ReleaseResponse = serde_json::from_str(&fixture("release_v4.json")).unwrap();
    assert_eq!(resp.published_at.as_deref(), Some("2023-10-16T17:17:35Z"));
}

// --- Releases list cache ---

#[test]
fn list_releases_offline_cache_hit() {
    use std::fs;

    let dir = TempDir::new().unwrap();
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
    let path = cache
        .path()
        .join("github")
        .join("actions")
        .join("checkout")
        .join("releases")
        .join("index.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    let index = actioneer::github::ReleasesIndex {
        releases: vec![actioneer::github::Release {
            tag_name: "v4.2.0".into(),
            published_at: "2021-06-01T00:00:00Z".into(),
            prerelease: false,
        }],
        fetched_at: 1_700_000_000,
    };
    fs::write(&path, serde_json::to_vec_pretty(&index).unwrap()).unwrap();

    let client = offline_client(&dir);
    let releases = client.list_releases("actions", "checkout").unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].tag_name, "v4.2.0");
}

// --- gh auth token (ignored unless gh is logged in) ---

#[test]
#[ignore = "requires gh cli logged in; run with --include-ignored"]
fn gh_auth_token_used_when_env_unset() {
    let gh_ok = std::process::Command::new("gh")
        .args(["auth", "status"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !gh_ok {
        eprintln!("skipping: gh not installed or not logged in");
        return;
    }

    // If GITHUB_TOKEN is set in the test runner env, this test cannot verify gh fallback.
    if std::env::var("GITHUB_TOKEN").is_ok_and(|t| !t.trim().is_empty()) {
        eprintln!("skipping: GITHUB_TOKEN is set in environment");
        return;
    }

    let token = actioneer::github::resolve_github_token();
    assert!(
        token.is_some(),
        "expected gh auth token when GITHUB_TOKEN unset and gh is logged in"
    );
}

// --- Live network test (ignored in CI) ---

#[test]
#[ignore = "requires live network access; run with --include-ignored"]
fn live_resolve_actions_checkout_v4() {
    let config = ActioneerConfig::default();
    let client = GitHubClient::new(&config, None);
    let resolved = client.resolve_ref("actions", "checkout", "v4").unwrap();
    assert_eq!(resolved.sha.len(), 40, "SHA must be 40 hex chars");
    assert!(
        resolved.sha.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA must be hex"
    );
    assert_eq!(resolved.ref_kind, RefKind::Tag);
}
