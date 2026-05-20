use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use walkdir::WalkDir;

use crate::model::Reference;
use crate::syntax::{collect_references, SyntaxError};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
}

pub fn scan(paths: &[String], recursive: bool) -> Result<Vec<Reference>, ScanError> {
    let mut found = Vec::new();
    for path in paths {
        let recursive = recursive || path == ".github";
        let input_path = Path::new(path);
        if !input_path.exists() {
            continue;
        }
        if input_path.is_dir() {
            scan_dir(input_path, recursive, &mut found)?;
        } else {
            scan_file(input_path, &mut found)?;
        }
    }
    Ok(found)
}

fn scan_dir(dir_path: &Path, recursive: bool, found: &mut Vec<Reference>) -> Result<(), ScanError> {
    if recursive {
        for entry in WalkDir::new(dir_path) {
            let entry = entry.map_err(std::io::Error::other)?;
            if !entry.file_type().is_file() || !is_yaml_file(entry.path()) {
                continue;
            }
            scan_file(entry.path(), found)?;
        }
        return Ok(());
    }
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_yaml_file(&path) {
            scan_file(&path, found)?;
        }
    }
    Ok(())
}

fn scan_file(file_path: &Path, found: &mut Vec<Reference>) -> Result<(), ScanError> {
    let contents = fs::read_to_string(file_path)?;
    let display_path = normalize_path(file_path);
    found.extend(collect_references(&display_path, &contents)?);
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    let raw = PathBuf::from(path);
    raw.to_string_lossy().replace('\\', "/")
}

fn is_yaml_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.ends_with(".yml") || name.ends_with(".yaml")
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn github_path_is_scanned_recursively_by_default() {
        let _guard = cwd_lock().lock().unwrap();
        let root = temp_test_dir("scanner-recursive");
        let github = root.join(".github");
        let nested = github.join("workflows");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("ci.yml"),
            concat!(
                "jobs:\n",
                "  build:\n",
                "    steps:\n",
                "      - uses: actions/checkout@v4\n",
            ),
        )
        .unwrap();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let found = scan(&[String::from(".github")], false).unwrap();

        assert_eq!(1, found.len());
        assert_eq!("actions/checkout", found[0].name.display());

        std::env::set_current_dir(previous).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn non_recursive_scan_skips_nested_yaml_for_regular_directories() {
        let root = temp_test_dir("scanner-non-recursive");
        let workflows = root.join("workflows");
        let nested = workflows.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("ci.yml"),
            concat!(
                "jobs:\n",
                "  build:\n",
                "    steps:\n",
                "      - uses: actions/checkout@v4\n",
            ),
        )
        .unwrap();

        let found = scan(&[workflows.display().to_string()], false).unwrap();

        assert!(found.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("actioneer-{label}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
