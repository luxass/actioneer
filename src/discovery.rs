//! Workflow file discovery under `.github/workflows/`.

use std::path::{Path, PathBuf};

/// Errors during workflow discovery.
#[derive(Debug)]
pub enum DiscoveryError {
    Io(std::io::Error),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to discover workflows: {e}"),
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for DiscoveryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Find workflow YAML files under `root/.github/workflows/`.
///
/// Returns paths sorted lexicographically for deterministic output.
/// Only regular files with `.yml` or `.yaml` extensions are included.
pub fn discover_workflows(root: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    let workflows_dir = root.join(".github").join("workflows");
    if !workflows_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&workflows_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext == "yml" || ext == "yaml");
        if is_yaml {
            paths.push(path);
        }
    }

    paths.sort();
    Ok(paths)
}
