use std::path::{Path, PathBuf};

/// The resolved cache directory for actioneer.
///
/// Obtained via [`cache_dir`]; wraps the resolved path for type-safety.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDir(PathBuf);

impl CacheDir {
    /// The resolved directory path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for CacheDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Resolve the actioneer cache directory.
///
/// Resolution order:
/// 1. `ACTIONEER_CACHE` environment variable (if set and non-empty)
/// 2. `$XDG_CACHE_HOME/actioneer` when `XDG_CACHE_HOME` is set (non-Windows)
/// 3. `$HOME/.cache/actioneer` on Unix/macOS, `%LOCALAPPDATA%\actioneer` on Windows
///
/// Returns `None` only when no home/cache directory can be determined.
pub fn cache_dir() -> Option<CacheDir> {
    resolve_cache_dir_with(std::env::var("ACTIONEER_CACHE").ok().as_deref())
}

/// Resolve the cache directory, accepting the `ACTIONEER_CACHE` value as an
/// explicit parameter so tests can call this without touching the process environment.
#[doc(hidden)]
pub fn resolve_cache_dir_with(env_override: Option<&str>) -> Option<CacheDir> {
    if let Some(p) = env_override.filter(|s| !s.is_empty()) {
        return Some(CacheDir(PathBuf::from(p)));
    }
    platform_cache_base().map(|p| CacheDir(p.join("actioneer")))
}

fn platform_cache_base() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME").filter(|s| !s.is_empty()) {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))
    }
}
