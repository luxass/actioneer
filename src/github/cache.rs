use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::actions::{Tag, parse_version};

const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6);

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
            .filter_map(|t| {
                Some(Tag {
                    name: t.name.clone(),
                    sha: t.sha,
                    version: parse_version(&t.name)?,
                })
            })
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
    matches!(std::env::var("ACTIONEER_NO_CACHE"), Ok(v) if matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}
