use std::fs;
use std::path::Path;

use serde_yaml::Value;
use walkdir::WalkDir;
use yamlpath::{Document, Feature, Route};

use crate::model::Action;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid yaml in {file}")]
    InvalidYaml { file: String },
}

pub fn scan(paths: &[String], recursive: bool) -> Result<Vec<Action>, ScanError> {
    let mut actions = Vec::new();
    for path in paths {
        let input = Path::new(path);
        if !input.exists() {
            continue;
        }
        if input.is_dir() {
            let recurse = recursive || path == ".github";
            if recurse {
                for entry in WalkDir::new(input) {
                    let entry = entry.map_err(std::io::Error::other)?;
                    if entry.file_type().is_file() && is_yaml(entry.path()) {
                        scan_file(entry.path(), &mut actions)?;
                    }
                }
            } else {
                for entry in fs::read_dir(input)? {
                    let p = entry?.path();
                    if p.is_file() && is_yaml(&p) {
                        scan_file(&p, &mut actions)?;
                    }
                }
            }
        } else {
            scan_file(input, &mut actions)?;
        }
    }
    Ok(actions)
}

fn is_yaml(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .unwrap_or(false)
}

fn scan_file(path: &Path, actions: &mut Vec<Action>) -> Result<(), ScanError> {
    let content = fs::read_to_string(path)?;
    let file = path.to_string_lossy().replace('\\', "/");

    let root: Value = serde_yaml::from_str(&content)
        .map_err(|_| ScanError::InvalidYaml { file: file.clone() })?;

    let doc = Document::new(content).map_err(|_| ScanError::InvalidYaml { file: file.clone() })?;

    if is_action_yml(&file) {
        collect_composite(&root, &doc, &file, actions);
    } else {
        collect_workflow(&root, &doc, &file, actions);
    }

    Ok(())
}

fn clean_scalar(value: &str) -> &str {
    let trimmed = value.trim_matches([' ', '\t']);
    if trimmed.len() >= 2 {
        let b = trimmed.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn extract_comment(doc: &Document, feature: &Feature<'_>) -> Option<String> {
    doc.feature_comments(feature)
        .into_iter()
        .filter(|c| c.location.point_span.0.0 == feature.location.point_span.0.0)
        .filter(|c| c.location.byte_span.0 >= feature.location.byte_span.1)
        .min_by_key(|c| c.location.byte_span.0)
        .and_then(|c| {
            let c = doc
                .extract(&c)
                .trim_start_matches('#')
                .trim_matches([' ', '\t'])
                .to_string();
            if c.is_empty() {
                None
            } else {
                extract_version(&c).map(|s| s.to_string())
            }
        })
}

fn extract_version(comment: &str) -> Option<&str> {
    comment
        .split(|c: char| {
            matches!(
                c,
                ' ' | '\t' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        })
        .map(|t| t.trim_matches(['.', ':']))
        .find(|t| {
            let rest = t
                .strip_prefix('v')
                .or_else(|| t.strip_prefix('V'))
                .unwrap_or(t);
            !rest.is_empty()
                && rest.as_bytes()[0].is_ascii_digit()
                && rest.chars().all(|c| c == '.' || c.is_ascii_digit())
        })
}

fn is_action_yml(file: &str) -> bool {
    file.ends_with("action.yml") || file.ends_with("action.yaml")
}

struct ParsedAction<'a> {
    owner: &'a str,
    name: &'a str,
    path: &'a str,
    r#ref: &'a str,
}

fn parse_action_ref(value: &str) -> Option<ParsedAction<'_>> {
    if value.starts_with("./") || value.starts_with("../") || value.starts_with("docker://") {
        return None;
    }
    let at = value.rfind('@')?;
    let action = &value[..at];
    let r#ref = &value[at + 1..];
    let mut parts = action.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty() || name.is_empty() || r#ref.is_empty() {
        return None;
    }
    let path = action.get(owner.len() + 1 + name.len()..).unwrap_or("");
    Some(ParsedAction {
        owner,
        name,
        path,
        r#ref,
    })
}

// --- tree walking with immediate Document queries ---

fn collect_workflow(root: &Value, doc: &Document, file: &str, actions: &mut Vec<Action>) {
    let Some(jobs) = root.get("jobs").and_then(Value::as_mapping) else {
        return;
    };
    let base = Route::default().with_key("jobs");
    for (k, v) in jobs
        .iter()
        .filter_map(|(k, v)| k.as_str().map(|n| (n.to_string(), v)))
    {
        if !v.is_mapping() {
            continue;
        }
        if v.get("uses").is_some() {
            push_action(
                doc,
                &base.with_key(k.clone()).with_key("uses"),
                file,
                actions,
            );
        }
        if let Some(steps) = v.get("steps").and_then(Value::as_sequence) {
            let steps_route = base.with_key(k.clone()).with_key("steps");
            for (i, step) in steps.iter().enumerate() {
                if step.get("uses").is_some() {
                    push_action(
                        doc,
                        &steps_route.with_key(i).with_key("uses"),
                        file,
                        actions,
                    );
                }
            }
        }
    }
}

fn collect_composite(root: &Value, doc: &Document, file: &str, actions: &mut Vec<Action>) {
    let Some(runs) = root.get("runs") else {
        return;
    };
    if runs.get("using").and_then(Value::as_str) != Some("composite") {
        return;
    }
    let Some(steps) = runs.get("steps").and_then(Value::as_sequence) else {
        return;
    };
    let base = Route::default().with_key("runs").with_key("steps");
    for (i, step) in steps.iter().enumerate() {
        if step.get("uses").is_some() {
            push_action(doc, &base.with_key(i).with_key("uses"), file, actions);
        }
    }
}

fn push_action(doc: &Document, route: &Route<'_>, file: &str, actions: &mut Vec<Action>) {
    let Some(feature) = doc.query_exact(route).ok().flatten() else {
        return;
    };
    let raw = doc.extract(&feature);
    let text = clean_scalar(raw);
    let leading = raw.find(text).unwrap_or(0);
    let value_start = feature.location.byte_span.0 + leading;
    let value_end = value_start + text.len();

    let Some(action) = parse_action_ref(text) else {
        return;
    };
    let comment = extract_comment(doc, &feature);

    let at = text.rfind('@').unwrap();
    let ref_start = value_start + at + 1;

    actions.push(Action::from_scan(
        action.owner.into(),
        action.name.into(),
        action.path.into(),
        action.r#ref.into(),
        comment,
        file.to_string(),
        feature.location.point_span.0.0 + 1,
        ref_start,
        value_end,
    ));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn github_dir_recursive_by_default() {
        let _guard = cwd_lock().lock().unwrap();
        let root = temp_dir("scan-recursive");
        let nested = root.join(".github").join("workflows");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("ci.yml"),
            "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
        )
        .unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let found = scan(&[".github".into()], false).unwrap();
        std::env::set_current_dir(prev).unwrap();
        assert_eq!(1, found.len());
        assert_eq!("v4", found[0].current_ref);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn non_recursive_skips_nested() {
        let root = temp_dir("scan-flat");
        let nested = root.join("wf").join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("ci.yml"),
            "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
        )
        .unwrap();
        let found = scan(&[root.join("wf").display().to_string()], false).unwrap();
        assert!(found.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_step_and_reusable_workflow() {
        let source = concat!(
            "jobs:\n",
            "  build:\n",
            "    uses: myorg/repo/.github/workflows/ci.yml@v1\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4 # v4.1.0\n",
            "      - uses: ./local-action\n",
        );
        let doc = Document::new(source.to_string()).unwrap();
        let root: Value = serde_yaml::from_str(source).unwrap();
        let mut actions = Vec::new();
        collect_workflow(&root, &doc, "ci.yml", &mut actions);
        assert_eq!(2, actions.len());
        assert_eq!("v1", actions[0].current_ref);
    }

    #[test]
    fn parses_quoted_ref_byte_positions() {
        let source = "uses: \"actions/setup-node@v4\"\n";
        let doc = Document::new(source.to_string()).unwrap();
        let feature = doc
            .query_exact(&Route::default().with_key("uses"))
            .unwrap()
            .unwrap();
        let raw = doc.extract(&feature);
        let text = clean_scalar(raw);
        let leading = raw.find(text).unwrap_or(0);
        let at = text.rfind('@').unwrap();
        let ref_start = feature.location.byte_span.0 + leading + at + 1;
        let ref_len = text[at + 1..].len();
        assert_eq!("v4", &source[ref_start..ref_start + ref_len]);
    }

    #[test]
    fn extracts_version_comment() {
        let source = "uses: actions/checkout@abc123 # v4.1.0\n";
        let doc = Document::new(source.to_string()).unwrap();
        let feature = doc
            .query_exact(&Route::default().with_key("uses"))
            .unwrap()
            .unwrap();
        let comment = extract_comment(&doc, &feature);
        assert_eq!(Some("v4.1.0".to_string()), comment);
    }

    #[test]
    fn ignores_local_and_docker() {
        assert!(parse_action_ref("./local-action").is_none());
        assert!(parse_action_ref("../shared-action").is_none());
        assert!(parse_action_ref("docker://alpine:3.20").is_none());
    }

    #[test]
    fn ignores_uses_in_string_values() {
        let source = concat!(
            "jobs:\n",
            "  build:\n",
            "    name: \"deploy: uses: actions/fake@v1\"\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4\n",
        );
        let doc = Document::new(source.to_string()).unwrap();
        let root: Value = serde_yaml::from_str(source).unwrap();
        let mut actions = Vec::new();
        collect_workflow(&root, &doc, "ci.yml", &mut actions);
        assert_eq!(1, actions.len());
        assert_eq!("actions/checkout", actions[0].action_name());
    }

    #[test]
    fn composite_action_steps() {
        let source = concat!(
            "name: Example\n",
            "runs:\n",
            "  using: composite\n",
            "  steps:\n",
            "    - uses: actions/setup-node@v4 # v4.0.0\n",
        );
        let doc = Document::new(source.to_string()).unwrap();
        let root: Value = serde_yaml::from_str(source).unwrap();
        let mut actions = Vec::new();
        collect_composite(&root, &doc, "action.yml", &mut actions);
        assert_eq!(1, actions.len());
        assert_eq!("actions/setup-node", actions[0].action_name());
        assert_eq!(Some("v4.0.0".to_string()), actions[0].version_comment);
    }

    fn temp_dir(label: &str) -> PathBuf {
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
