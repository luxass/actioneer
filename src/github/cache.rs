//! On-disk cache for GitHub API responses.
//!
//! Cache layout under [`CacheDir`]:
//!
//! ```text
//! <cache_dir>/github/
//!   <owner>/<repo>/
//!     refs/
//!       tags/<encoded_tag>.json
//!       heads/<encoded_branch>.json
//!     releases/
//!       index.json
//! ```
//!
//! Branch names that contain `/` (e.g. `feature/foo`) are encoded as
//! `feature%2Ffoo` in the filename so that the file stays within the
//! `heads/` directory. All other characters are preserved as-is.
//!
//! Ref files store a [`CacheEntry`]; the release index stores a
//! [`ReleasesIndex`](super::ReleasesIndex). Writes are performed atomically: JSON
//! is written to `<path>.tmp`, then renamed to `<path>`.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::cache::CacheDir;

use super::{GitHubError, RefKind, ReleasesIndex};

/// A single on-disk cache entry for a resolved GitHub ref.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Full 40-character commit SHA.
    pub sha: String,
    /// How the ref was classified when it was fetched: `"tag"`, `"branch"`, or `"sha"`.
    pub ref_kind: String,
    /// ISO 8601 publication timestamp from the GitHub Releases API, or `null`.
    pub published_at: Option<String>,
    /// Unix timestamp (seconds) when this entry was written to the cache.
    pub fetched_at: u64,
}

impl CacheEntry {
    /// Reconstruct the [`RefKind`] from the stored string.
    pub fn ref_kind(&self) -> RefKind {
        match self.ref_kind.as_str() {
            "tag" => RefKind::Tag,
            "sha" => RefKind::Sha,
            _ => RefKind::Branch,
        }
    }
}

/// Compute the cache path for a resolved ref entry.
///
/// `kind` is `"tags"` or `"heads"`. The ref string is `/`-encoded so that
/// branch names with slashes remain within the `heads/` directory.
pub(super) fn ref_path(
    cache: &CacheDir,
    owner: &str,
    repo: &str,
    kind: &str,
    git_ref: &str,
) -> PathBuf {
    let encoded = encode_ref(git_ref);
    cache
        .path()
        .join("github")
        .join(owner)
        .join(repo)
        .join("refs")
        .join(kind)
        .join(format!("{encoded}.json"))
}

/// Compute the cache path for a repository releases index.
pub(super) fn releases_index_path(cache: &CacheDir, owner: &str, repo: &str) -> PathBuf {
    cache
        .path()
        .join("github")
        .join(owner)
        .join(repo)
        .join("releases")
        .join("index.json")
}

/// Write a [`ReleasesIndex`] to `path` atomically.
pub(super) fn write_releases_index(path: &Path, index: &ReleasesIndex) -> Result<(), GitHubError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(GitHubError::CacheWrite)?;
    }
    let json = serde_json::to_vec_pretty(index).map_err(GitHubError::Json)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json).map_err(GitHubError::CacheWrite)?;
    fs::rename(&tmp, path).map_err(GitHubError::CacheWrite)?;
    Ok(())
}

/// Read a [`CacheEntry`] from `path`.
///
/// Returns `Ok(None)` if the file does not exist.
pub(super) fn read_entry(path: &Path) -> Result<Option<CacheEntry>, GitHubError> {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(GitHubError::CacheRead(e)),
    };
    let entry: CacheEntry = serde_json::from_slice(&data).map_err(GitHubError::Json)?;
    Ok(Some(entry))
}

/// Write a [`CacheEntry`] to `path` atomically.
///
/// Parent directories are created as needed. The file is written to
/// `<path>.tmp` and then renamed to `<path>` to ensure atomicity on
/// POSIX systems (both paths are on the same filesystem).
pub(super) fn write_entry(path: &Path, entry: &CacheEntry) -> Result<(), GitHubError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(GitHubError::CacheWrite)?;
    }
    let json = serde_json::to_vec_pretty(entry).map_err(GitHubError::Json)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json).map_err(GitHubError::CacheWrite)?;
    fs::rename(&tmp, path).map_err(GitHubError::CacheWrite)?;
    Ok(())
}

/// Return the current Unix timestamp in seconds.
pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Encode a ref string for use as a file-system path component.
///
/// Only `/` is encoded (as `%2F`). Other characters used in valid git ref
/// names (letters, digits, `-`, `_`, `.`) are left unchanged.
fn encode_ref(s: &str) -> String {
    s.replace('/', "%2F")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_cache() -> (TempDir, CacheDir) {
        use crate::cache::resolve_cache_dir_with;
        let dir = TempDir::new().unwrap();
        let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
        (dir, cache)
    }

    #[test]
    fn encode_simple_tag() {
        assert_eq!(encode_ref("v4"), "v4");
        assert_eq!(encode_ref("v1.2.3"), "v1.2.3");
    }

    #[test]
    fn encode_branch_with_slash() {
        assert_eq!(encode_ref("feature/foo"), "feature%2Ffoo");
    }

    #[test]
    fn round_trip_entry() {
        let (_dir, cache) = temp_cache();
        let path = ref_path(&cache, "actions", "checkout", "tags", "v4");
        let entry = CacheEntry {
            sha: "a81bbbf8298c0fa03ea29cdc473d45769f953675".into(),
            ref_kind: "tag".into(),
            published_at: Some("2023-10-16T00:00:00Z".into()),
            fetched_at: 1_700_000_000,
        };
        write_entry(&path, &entry).unwrap();
        let loaded = read_entry(&path).unwrap().unwrap();
        assert_eq!(loaded, entry);
    }

    #[test]
    fn read_missing_returns_none() {
        let (_dir, cache) = temp_cache();
        let path = ref_path(&cache, "no-owner", "no-repo", "tags", "nonexistent");
        assert!(read_entry(&path).unwrap().is_none());
    }

    #[test]
    fn write_creates_parents() {
        let (_dir, cache) = temp_cache();
        let path = ref_path(&cache, "a", "b", "heads", "feature/deep/branch");
        let entry = CacheEntry {
            sha: "0".repeat(40),
            ref_kind: "branch".into(),
            published_at: None,
            fetched_at: 0,
        };
        write_entry(&path, &entry).unwrap();
        assert!(path.exists());
    }
}
