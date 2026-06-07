use std::fs;
use std::path::Path;

use serde_yaml::Value;
use walkdir::WalkDir;
use yamlpath::{Document, Feature, Route};

use crate::actions::ActionReference;

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid yaml in {file}")]
    InvalidYaml { file: String },
}

pub fn find_action_references(
    paths: &[String],
    recursive: bool,
) -> Result<Vec<ActionReference>, DiscoveryError> {
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
                    if entry.file_type().is_file() && is_yaml_file(entry.path()) {
                        scan_file(entry.path(), &mut actions)?;
                    }
                }
            } else {
                for entry in fs::read_dir(input)? {
                    let p = entry?.path();
                    if p.is_file() && is_yaml_file(&p) {
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

fn is_yaml_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .unwrap_or(false)
}

fn scan_file(path: &Path, actions: &mut Vec<ActionReference>) -> Result<(), DiscoveryError> {
    let content = fs::read_to_string(path)?;
    let file = path.to_string_lossy().replace('\\', "/");

    let root: Value = serde_yaml::from_str(&content)
        .map_err(|_| DiscoveryError::InvalidYaml { file: file.clone() })?;

    let doc =
        Document::new(content).map_err(|_| DiscoveryError::InvalidYaml { file: file.clone() })?;

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

fn collect_workflow(root: &Value, doc: &Document, file: &str, actions: &mut Vec<ActionReference>) {
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

fn collect_composite(root: &Value, doc: &Document, file: &str, actions: &mut Vec<ActionReference>) {
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

fn push_action(doc: &Document, route: &Route<'_>, file: &str, actions: &mut Vec<ActionReference>) {
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

    actions.push(ActionReference::from_discovery(
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
    use super::*;

    #[test]
    fn parse_action_ref_standard() {
        let p = parse_action_ref("actions/checkout@v4").unwrap();
        assert_eq!("actions", p.owner);
        assert_eq!("checkout", p.name);
        assert_eq!("", p.path);
        assert_eq!("v4", p.r#ref);
    }

    #[test]
    fn parse_action_ref_with_path() {
        let p = parse_action_ref("myorg/repo/.github/workflows/ci.yml@main").unwrap();
        assert_eq!("myorg", p.owner);
        assert_eq!("repo", p.name);
        assert_eq!("/.github/workflows/ci.yml", p.path);
        assert_eq!("main", p.r#ref);
    }

    #[test]
    fn parse_action_ref_missing_at() {
        assert!(parse_action_ref("actions/checkout").is_none());
    }

    #[test]
    fn parse_action_ref_local_dot_slash() {
        assert!(parse_action_ref("./local-action").is_none());
    }

    #[test]
    fn parse_action_ref_local_dot_dot() {
        assert!(parse_action_ref("../shared-action").is_none());
    }

    #[test]
    fn parse_action_ref_docker() {
        assert!(parse_action_ref("docker://alpine:3.20").is_none());
    }

    #[test]
    fn parse_action_ref_empty_parts() {
        assert!(parse_action_ref("/name@v1").is_none());
        assert!(parse_action_ref("owner/@v1").is_none());
    }

    #[test]
    fn parse_action_ref_sha() {
        let p =
            parse_action_ref("actions/checkout@abcdef0123456789abcdef0123456789abcdef01").unwrap();
        assert_eq!("abcdef0123456789abcdef0123456789abcdef01", p.r#ref);
    }

    #[test]
    fn clean_scalar_plain() {
        assert_eq!("hello", clean_scalar("hello"));
    }

    #[test]
    fn clean_scalar_double_quoted() {
        assert_eq!("hello@v1", clean_scalar("\"hello@v1\""));
    }

    #[test]
    fn clean_scalar_single_quoted() {
        assert_eq!("hello", clean_scalar("'hello'"));
    }

    #[test]
    fn clean_scalar_whitespace() {
        assert_eq!("hello", clean_scalar("  hello  "));
    }

    #[test]
    fn extract_version_v_prefix() {
        assert_eq!(Some("v4.1.0"), extract_version("# v4.1.0 trail"));
    }

    #[test]
    fn extract_version_no_prefix() {
        assert_eq!(Some("1.2.3"), extract_version(" 1.2.3 "));
    }

    #[test]
    fn extract_version_non_version_text() {
        assert!(extract_version("just a comment").is_none());
    }

    #[test]
    fn extract_version_empty() {
        assert!(extract_version("").is_none());
    }

    #[test]
    fn is_yaml_yml() {
        assert!(is_yaml_file(Path::new("ci.yml")));
    }

    #[test]
    fn is_yaml_yaml() {
        assert!(is_yaml_file(Path::new("ci.yaml")));
    }

    #[test]
    fn is_yaml_other() {
        assert!(!is_yaml_file(Path::new("ci.json")));
        assert!(!is_yaml_file(Path::new("ci.txt")));
    }

    #[test]
    fn is_action_yml_true() {
        assert!(is_action_yml("action.yml"));
        assert!(is_action_yml("action.yaml"));
    }

    #[test]
    fn is_action_yml_false() {
        assert!(!is_action_yml("workflow.yml"));
    }

    #[test]
    fn extract_comment_finds_version() {
        let source = "uses: actions/checkout@abc123 # v4.1.0\n";
        let doc = Document::new(source.to_string()).unwrap();
        let feature = doc
            .query_exact(&Route::default().with_key("uses"))
            .unwrap()
            .unwrap();
        assert_eq!(Some("v4.1.0".to_string()), extract_comment(&doc, &feature));
    }

    #[test]
    fn extract_comment_none_when_missing() {
        let source = "uses: actions/checkout@v4\n";
        let doc = Document::new(source.to_string()).unwrap();
        let feature = doc
            .query_exact(&Route::default().with_key("uses"))
            .unwrap()
            .unwrap();
        assert_eq!(None, extract_comment(&doc, &feature));
    }
}
