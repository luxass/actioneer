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
    pub version_comment: Option<String>,
}

pub fn filter_action_refs(
    references: Vec<ActionRef>,
    include: &[String],
    exclude: &[String],
) -> Vec<ActionRef> {
    references
        .into_iter()
        .filter(|action_ref| {
            if !include.is_empty() && !include.contains(&action_ref.repo) {
                return false;
            }
            let full_name = if action_ref.path.is_empty() {
                action_ref.repo.clone()
            } else {
                format!("{}/{}", action_ref.repo, action_ref.path)
            };
            !exclude.iter().any(|pattern| full_name.contains(pattern))
        })
        .collect()
}

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
            let Some((uses_value, version_comment)) = extract_uses(line) else {
                continue;
            };
            let Some(action_ref) = parse_action_ref(&file, index + 1, &uses_value, version_comment) else {
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

fn extract_uses(line: &str) -> Option<(String, Option<String>)> {
    let trimmed = line.trim_start();
    let rest = if let Some(rest) = trimmed.strip_prefix("uses:") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("- uses:") {
        rest
    } else {
        return None;
    };

    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }

    let (uses_value, tail) = if let Some(quoted) = rest.strip_prefix('"') {
        let value = quoted.split('"').next().map(str::to_string)?;
        let tail = quoted
            .split_once('"')
            .map(|(_, tail)| tail)
            .unwrap_or(quoted);
        (value, tail)
    } else if let Some(quoted) = rest.strip_prefix('\'') {
        let value = quoted.split('\'').next().map(str::to_string)?;
        let tail = quoted
            .split_once('\'')
            .map(|(_, tail)| tail)
            .unwrap_or(quoted);
        (value, tail)
    } else {
        let value = rest
            .split_whitespace()
            .next()
            .map(|raw| raw.trim_end_matches(',').to_string())?;
        let tail = rest[value.len()..].trim_start();
        (value, tail)
    };

    let version_comment = tail
        .split_once('#')
        .map(|(_, comment)| comment.trim().to_string())
        .filter(|comment| !comment.is_empty());

    Some((uses_value, version_comment))
}

fn parse_action_ref(
    file: &Path,
    line: usize,
    value: &str,
    version_comment: Option<String>,
) -> Option<ActionRef> {
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
        version_comment,
    })
}
