//! Workflow file discovery.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Errors during workflow discovery.
#[derive(Debug)]
pub enum DiscoveryError {
    /// A directory or filesystem entry could not be read.
    Io(std::io::Error),
    /// An explicit target does not exist.
    NotFound {
        /// Target path as supplied by the caller.
        path: PathBuf,
    },
    /// An explicit file target is not YAML.
    NotWorkflowFile {
        /// Target path as supplied by the caller.
        path: PathBuf,
    },
    /// An explicit target exists but is neither a regular file nor directory.
    NotFileOrDirectory {
        /// Target path as supplied by the caller.
        path: PathBuf,
    },
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to discover workflows: {e}"),
            Self::NotFound { path } => write!(f, "workflow path not found: {}", path.display()),
            Self::NotWorkflowFile { path } => {
                write!(
                    f,
                    "not a workflow file (expected .yml or .yaml): {}",
                    path.display()
                )
            }
            Self::NotFileOrDirectory { path } => {
                write!(f, "not a file or directory: {}", path.display())
            }
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
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
/// Returns paths relative to `root`, sorted lexicographically.
pub fn discover_workflows(root: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    resolve_workflow_paths(root, &[])
}

/// Resolve explicit workflow targets or fall back to the default repo layout.
///
/// - `targets` empty → `root/.github/workflows/*.{yml,yaml}` (flat)
/// - file target → that file (must be `.yml` / `.yaml`)
/// - directory target → `*.{yml,yaml}` directly in that directory (flat, no recursion)
///
/// Returns paths relative to `root`, sorted and deduplicated.
pub fn resolve_workflow_paths(
    root: &Path,
    targets: &[PathBuf],
) -> Result<Vec<PathBuf>, DiscoveryError> {
    if targets.is_empty() {
        return discover_default_workflows(root);
    }

    let mut paths = BTreeSet::new();
    for target in targets {
        let absolute = if target.is_absolute() {
            target.clone()
        } else {
            root.join(target)
        };

        if !absolute.exists() {
            return Err(DiscoveryError::NotFound {
                path: target.clone(),
            });
        }

        if absolute.is_file() {
            if !is_yaml(&absolute) {
                return Err(DiscoveryError::NotWorkflowFile {
                    path: target.clone(),
                });
            }
            paths.insert(relative_to_root(root, &absolute));
        } else if absolute.is_dir() {
            for path in yaml_files_in_dir(&absolute)? {
                paths.insert(relative_to_root(root, &path));
            }
        } else {
            return Err(DiscoveryError::NotFileOrDirectory {
                path: target.clone(),
            });
        }
    }

    Ok(paths.into_iter().collect())
}

fn discover_default_workflows(root: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    let workflows_dir = root.join(".github").join("workflows");
    if !workflows_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut paths = yaml_files_in_dir(&workflows_dir)?
        .into_iter()
        .map(|p| relative_to_root(root, &p))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn yaml_files_in_dir(dir: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_yaml(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn is_yaml(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext == "yml" || ext == "yaml")
}

fn relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}
