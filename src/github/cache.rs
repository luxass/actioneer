use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::git::parse_version;
use crate::model::Repository;

use super::Tag;

#[derive(Debug, Deserialize, Serialize)]
struct CachedTag {
    name: String,
    sha: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CacheEntry {
    pub(crate) etag: Option<String>,
    fetched_at: u64,
    tags: Vec<CachedTag>,
}

impl CacheEntry {
    pub(crate) fn from_tags(tags: &[Tag], etag: Option<String>, now: SystemTime) -> Self {
        let fetched_at = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        Self {
            etag,
            fetched_at,
            tags: tags
                .iter()
                .map(|tag| CachedTag {
                    name: tag.name.clone(),
                    sha: tag.sha.clone(),
                })
                .collect(),
        }
    }

    pub(crate) fn into_tags(self) -> Vec<Tag> {
        self.tags
            .into_iter()
            .filter_map(|tag| {
                let version = parse_version(&tag.name)?;
                Some(Tag {
                    name: tag.name,
                    sha: tag.sha,
                    version,
                })
            })
            .collect()
    }
}

pub(crate) fn no_cache_from_env() -> bool {
    matches!(
        std::env::var("ACTIONEER_NO_CACHE"),
        Ok(value) if matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    )
}

pub(crate) fn cache_file_path(repository: &Repository) -> PathBuf {
    std::env::temp_dir()
        .join("actioneer-cache")
        .join("tags")
        .join(format!("{}__{}.json", repository.owner, repository.name))
}

pub(crate) fn read_cached_tags(path: &Path) -> Option<CacheEntry> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub(crate) fn write_cached_tags(
    path: &Path,
    tags: &[Tag],
    etag: Option<String>,
    now: SystemTime,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let entry = CacheEntry::from_tags(tags, etag, now);
    let contents = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::engine::git::Version;
    use crate::model::Repository;

    use super::{
        cache_file_path, no_cache_from_env, read_cached_tags, write_cached_tags, CacheEntry, Tag,
    };

    #[test]
    fn no_cache_env_recognizes_common_truthy_values() {
        std::env::set_var("ACTIONEER_NO_CACHE", "1");
        assert!(no_cache_from_env());
        std::env::set_var("ACTIONEER_NO_CACHE", "true");
        assert!(no_cache_from_env());
        std::env::set_var("ACTIONEER_NO_CACHE", "yes");
        assert!(no_cache_from_env());
        std::env::remove_var("ACTIONEER_NO_CACHE");
    }

    #[test]
    fn cache_file_path_uses_temp_dir_and_repository_name() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };

        let path = cache_file_path(&repository);

        assert!(path.ends_with("actioneer-cache/tags/actions__checkout.json"));
    }

    #[test]
    fn cached_tags_round_trip_with_etag() {
        let temp = std::env::temp_dir().join(format!(
            "actioneer-cache-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = temp.join("tags.json");
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let tags = vec![Tag {
            name: "v1.2.3".into(),
            sha: "abc123".into(),
            version: Version {
                major: 1,
                minor: 2,
                patch: 3,
            },
        }];

        write_cached_tags(&path, &tags, Some(String::from("\"etag-1\"")), now).unwrap();
        let cached = read_cached_tags(&path).unwrap();

        assert_eq!(Some(String::from("\"etag-1\"")), cached.etag.clone());
        assert_eq!(tags, cached.into_tags());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn cache_entry_without_etag_still_round_trips_tags() {
        let entry = CacheEntry::from_tags(
            &[Tag {
                name: "v1.2.3".into(),
                sha: "abc123".into(),
                version: Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                },
            }],
            None,
            UNIX_EPOCH + Duration::from_secs(1_000),
        );

        assert_eq!(None, entry.etag.clone());
        assert_eq!(1, entry.into_tags().len());
    }
}
