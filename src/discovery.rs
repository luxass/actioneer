use std::{fs, path::Path};

use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRef {
    pub file: std::path::PathBuf,
    pub line: usize,
    pub owner: String,
    pub name: String,
    pub repo: String,
    pub path: String,
    pub ref_name: String,
}

pub type DiscoveredActionRef = ActionRef;

pub fn discover_action_refs<I, P>(inputs: I) -> Result<Vec<ActionRef>, String>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut files = Vec::new();
    for input in inputs {
        collect_yaml_files(input.as_ref(), &mut files)?;
    }
    files.sort();

    let mut refs = Vec::new();
    for file in files {
        let contents = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {}: {error}", file.display()))?;

        for (index, line) in contents.lines().enumerate() {
            let Some(uses_value) = extract_uses_value(line) else {
                continue;
            };
            let Some(action_ref) = parse_action_ref(&file, index + 1, &uses_value) else {
                continue;
            };
            refs.push(action_ref);
        }
    }

    Ok(refs)
}

fn collect_yaml_files(path: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_yaml_file(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if path.is_dir() {
        for entry in WalkDir::new(path) {
            let entry =
                entry.map_err(|error| format!("failed to scan {}: {error}", path.display()))?;
            let entry_path = entry.path();
            if entry_path.is_file() && is_yaml_file(entry_path) {
                files.push(entry_path.to_path_buf());
            }
        }
    }

    Ok(())
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("action.yml" | "action.yaml")
    ) || matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yml" | "yaml")
    )
}

fn extract_uses_value(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let value = if let Some(rest) = trimmed.strip_prefix("uses:") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("- uses:") {
        rest
    } else {
        return None;
    };

    let value = value.trim_start();
    if value.is_empty() {
        return None;
    }

    if let Some(quoted) = value.strip_prefix('"') {
        return quoted.split('"').next().map(str::to_string);
    }
    if let Some(quoted) = value.strip_prefix('\'') {
        return quoted.split('\'').next().map(str::to_string);
    }

    value
        .split_whitespace()
        .next()
        .map(|raw| raw.trim_end_matches(',').to_string())
}

fn parse_action_ref(file: &Path, line: usize, value: &str) -> Option<ActionRef> {
    if value.starts_with("./") || value.starts_with("../") || value.starts_with("docker://") {
        return None;
    }

    let (action, ref_name) = value.split_once('@')?;
    let parts = action.split('/').collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }

    let owner = parts[0].to_string();
    let name = parts[1].to_string();
    let repo = format!("{owner}/{name}");
    let path = parts.get(2..).unwrap_or_default().join("/");

    Some(ActionRef {
        file: file.to_path_buf(),
        line,
        owner,
        name,
        repo,
        path,
        ref_name: ref_name.to_string(),
    })
}
