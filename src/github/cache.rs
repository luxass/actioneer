use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::actions::Tag;

const CACHE_TTL: Duration = Duration::from_secs(60 * 5);

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedTag {
    name: String,
    sha: String,
}

pub fn cache_path(owner: &str, name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("actioneer-cache")
        .join("tags")
        .join(format!(
            "{}__{}.json",
            encode_cache_component(owner),
            encode_cache_component(name)
        ))
}

fn encode_cache_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub fn read_cache(path: &Path) -> Option<Vec<Tag>> {
    let meta = fs::metadata(path).ok()?;
    let age = SystemTime::now()
        .duration_since(meta.modified().ok()?)
        .ok()?;
    if age > CACHE_TTL {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    let cached: Vec<CachedTag> = serde_json::from_str(&contents).ok()?;
    Some(
        cached
            .into_iter()
            .filter_map(|t| Tag::from_name_sha(t.name, t.sha))
            .collect(),
    )
}

pub fn write_cache(path: &Path, tags: &[Tag]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let cached: Vec<CachedTag> = tags
        .iter()
        .map(|t| CachedTag {
            name: t.name.clone(),
            sha: t.sha.clone(),
        })
        .collect();
    if let Ok(json) = serde_json::to_string(&cached) {
        let _ = fs::write(path, json);
    }
}

pub fn no_cache_from_env() -> bool {
    std::env::var("ACTIONEER_NO_CACHE")
        .ok()
        .is_some_and(|v| is_truthy(&v))
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::parse_version;

    #[test]
    fn encode_keeps_safe_characters() {
        assert_eq!("actions", encode_cache_component("actions"));
        assert_eq!("my-repo_v1.2", encode_cache_component("my-repo_v1.2"));
    }

    #[test]
    fn encode_escapes_unsafe_characters() {
        assert_eq!("a%2Fb", encode_cache_component("a/b"));
        assert_eq!("owner%20name", encode_cache_component("owner name"));
    }

    #[test]
    fn encode_escapes_path_traversal() {
        let encoded = encode_cache_component("../../etc/passwd");
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn cache_path_is_namespaced_per_repo() {
        let a = cache_path("actions", "checkout");
        let b = cache_path("actions", "setup-node");
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with("actions__checkout.json"));
    }

    #[test]
    fn read_cache_missing_file_returns_none() {
        let path = std::env::temp_dir().join("actioneer-cache-test-does-not-exist.json");
        assert!(read_cache(&path).is_none());
    }

    #[test]
    fn write_then_read_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "actioneer-cache-test-roundtrip-{}.json",
            std::process::id()
        ));
        let tags = vec![Tag {
            name: "v1.2.3".into(),
            sha: "abc123".into(),
            version: parse_version("v1.2.3").unwrap(),
        }];

        write_cache(&path, &tags);
        let cached = read_cache(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(1, cached.len());
        assert_eq!("v1.2.3", cached[0].name);
        assert_eq!("abc123", cached[0].sha);
        assert_eq!(tags[0].version, cached[0].version);
    }

    #[test]
    fn read_cache_drops_unparseable_versions() {
        let path = std::env::temp_dir().join(format!(
            "actioneer-cache-test-unparseable-{}.json",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"[{"name":"not-a-version","sha":"abc"},{"name":"v2.0.0","sha":"def"}]"#,
        )
        .unwrap();

        let cached = read_cache(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(1, cached.len());
        assert_eq!("v2.0.0", cached[0].name);
    }

    #[test]
    fn read_cache_expired_returns_none() {
        let path = std::env::temp_dir().join(format!(
            "actioneer-cache-test-expired-{}.json",
            std::process::id()
        ));
        fs::write(&path, "[]").unwrap();
        let stale = SystemTime::now() - (CACHE_TTL + Duration::from_secs(60));
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(stale))
            .unwrap();
        drop(file);

        let cached = read_cache(&path);
        let _ = fs::remove_file(&path);

        assert!(cached.is_none());
    }

    #[test]
    fn read_cache_invalid_json_returns_none() {
        let path = std::env::temp_dir().join(format!(
            "actioneer-cache-test-invalid-{}.json",
            std::process::id()
        ));
        fs::write(&path, "not json").unwrap();

        let cached = read_cache(&path);
        let _ = fs::remove_file(&path);

        assert!(cached.is_none());
    }

    #[test]
    fn truthy_values() {
        for v in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(is_truthy(v), "{v:?} should be truthy");
        }
        for v in ["", "0", "false", "no", "off", "2"] {
            assert!(!is_truthy(v), "{v:?} should be falsy");
        }
    }
}
