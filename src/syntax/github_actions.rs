use serde_yaml::Value;
use thiserror::Error;
use yamlpath::{Document, Feature, Route};

use crate::model::{ActionName, ByteSpan, Reference, ReferenceKind, Repository, SourceLocation};

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("invalid yaml")]
    InvalidYaml,
}

pub fn collect_references(file_path: &str, contents: &str) -> Result<Vec<Reference>, SyntaxError> {
    let document = parse_document(contents)?;
    let root: Value = serde_yaml::from_str(contents).map_err(|_| SyntaxError::InvalidYaml)?;
    let mut found = Vec::new();
    if is_composite_action(file_path) {
        collect_composite_actions(&document, &root, file_path, &mut found);
    } else {
        collect_workflow_actions(&document, &root, file_path, &mut found);
    }
    Ok(found)
}

fn collect_workflow_actions(
    document: &Document,
    root: &Value,
    file_path: &str,
    found: &mut Vec<Reference>,
) {
    let Some(jobs) = mapping_value(root, "jobs").and_then(Value::as_mapping) else {
        return;
    };
    let jobs_route = Route::default().with_key("jobs");
    for (job_name, job_value) in jobs
        .iter()
        .filter_map(|(key, value)| key.as_str().map(|job_name| (job_name.to_string(), value)))
    {
        if !job_value.is_mapping() {
            continue;
        }
        collect_job_actions(
            document,
            job_value,
            jobs_route.with_key(job_name.clone()),
            file_path,
            &job_name,
            found,
        );
    }
}

fn collect_job_actions(
    document: &Document,
    job: &Value,
    job_route: Route<'_>,
    file_path: &str,
    job_scope: &str,
    found: &mut Vec<Reference>,
) {
    if mapping_value(job, "uses").is_some() {
        append_action_reference(
            document,
            &job_route.with_key("uses"),
            job_scope,
            ReferenceKind::WorkflowJob,
            file_path,
            found,
        );
    }
    if let Some(steps) = mapping_value(job, "steps").and_then(Value::as_sequence) {
        let steps_route = job_route.with_key("steps");
        collect_step_actions(
            document,
            steps,
            &steps_route,
            file_path,
            job_scope,
            ReferenceKind::WorkflowStep,
            found,
        );
    }
}

fn collect_composite_actions(
    document: &Document,
    root: &Value,
    file_path: &str,
    found: &mut Vec<Reference>,
) {
    let Some(runs) = mapping_value(root, "runs").and_then(Value::as_mapping) else {
        return;
    };
    let Some(using) = runs
        .get(Value::String("using".into()))
        .and_then(Value::as_str)
    else {
        return;
    };
    if using != "composite" {
        return;
    }
    let Some(steps) = runs
        .get(Value::String("steps".into()))
        .and_then(Value::as_sequence)
    else {
        return;
    };
    collect_step_actions(
        document,
        steps,
        &Route::default().with_key("runs").with_key("steps"),
        file_path,
        "composite",
        ReferenceKind::CompositeStep,
        found,
    );
}

fn collect_step_actions(
    document: &Document,
    steps: &[Value],
    steps_route: &Route<'_>,
    file_path: &str,
    scope: &str,
    kind: ReferenceKind,
    found: &mut Vec<Reference>,
) {
    for (index, step) in steps.iter().enumerate() {
        if mapping_value(step, "uses").is_none() {
            continue;
        }
        append_action_reference(
            document,
            &steps_route.with_key(index).with_key("uses"),
            scope,
            kind.clone(),
            file_path,
            found,
        );
    }
}

fn append_action_reference(
    document: &Document,
    route: &Route<'_>,
    scope: &str,
    kind: ReferenceKind,
    file_path: &str,
    found: &mut Vec<Reference>,
) {
    let Some(value_range) = scalar_at_route(document, route).ok().flatten() else {
        return;
    };
    if let Some(reference) = action_from_uses_value(
        value_range.text,
        &value_range.trailing_comment,
        scope,
        kind,
        file_path,
        value_range.line,
        value_range.start_byte,
        value_range.end_byte,
    ) {
        found.push(reference);
    }
}

fn mapping_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_mapping()?.get(Value::String(key.to_string()))
}

#[derive(Debug)]
struct ScalarRange<'a> {
    text: &'a str,
    start_byte: usize,
    end_byte: usize,
    line: usize,
    trailing_comment: String,
}

fn parse_document(contents: &str) -> Result<Document, SyntaxError> {
    Document::new(contents.to_string()).map_err(|_| SyntaxError::InvalidYaml)
}

fn scalar_at_route<'a>(
    document: &'a Document,
    route: &Route<'_>,
) -> Result<Option<ScalarRange<'a>>, SyntaxError> {
    let Some(feature) = document
        .query_exact(route)
        .map_err(|_| SyntaxError::InvalidYaml)?
    else {
        return Ok(None);
    };

    let raw = document.extract(&feature);
    let text = clean_scalar(raw);
    let leading = raw.find(text).unwrap_or(0);
    let start_byte = feature.location.byte_span.0 + leading;
    let end_byte = start_byte + text.len();
    let line = feature.location.point_span.0 .0 + 1;
    let trailing_comment = trailing_comment(document, &feature);

    Ok(Some(ScalarRange {
        text,
        start_byte,
        end_byte,
        line,
        trailing_comment,
    }))
}

fn trailing_comment(document: &Document, feature: &Feature<'_>) -> String {
    document
        .feature_comments(feature)
        .into_iter()
        .filter(|comment| comment.location.point_span.0 .0 == feature.location.point_span.0 .0)
        .filter(|comment| comment.location.byte_span.0 >= feature.location.byte_span.1)
        .min_by_key(|comment| comment.location.byte_span.0)
        .map(|comment| {
            document
                .extract(&comment)
                .trim_start_matches('#')
                .trim_matches([' ', '\t'])
                .to_string()
        })
        .unwrap_or_default()
}

fn clean_scalar(value: &str) -> &str {
    let trimmed = value.trim_matches([' ', '\t']);
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn action_from_uses_value(
    value: &str,
    comment: &str,
    scope: &str,
    kind: ReferenceKind,
    file_path: &str,
    line: usize,
    value_start: usize,
    value_end: usize,
) -> Option<Reference> {
    if value.starts_with("./") || value.starts_with("../") || value.starts_with("docker://") {
        return None;
    }
    let parsed = parse_action_ref(value)?;
    let version_comment = extract_version_comment(comment).unwrap_or_default();
    let ref_start = value_start + parsed.action.len() + 1;
    Some(Reference {
        kind,
        name: ActionName {
            repository: Repository {
                owner: parsed.owner.to_string(),
                name: parsed.name.to_string(),
            },
            path: parsed.path.to_string(),
        },
        current_ref: parsed.r#ref.to_string(),
        version_hint: version_comment.to_string(),
        scope: scope.to_string(),
        source: SourceLocation {
            file: file_path.to_string(),
            line,
            ref_span: ByteSpan {
                start: ref_start,
                end: value_end,
            },
        },
    })
}

struct ParsedActionRef<'a> {
    action: &'a str,
    owner: &'a str,
    name: &'a str,
    path: &'a str,
    r#ref: &'a str,
}

fn parse_action_ref(value: &str) -> Option<ParsedActionRef<'_>> {
    let at = value.rfind('@')?;
    let action = &value[..at];
    let r#ref = &value[at + 1..];
    let mut parts = action.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty() || name.is_empty() || r#ref.is_empty() {
        return None;
    }
    let path_start = owner.len() + 1 + name.len();
    Some(ParsedActionRef {
        action,
        owner,
        name,
        path: action.get(path_start..).unwrap_or_default(),
        r#ref,
    })
}

fn extract_version_comment(comment: &str) -> Option<&str> {
    if comment.is_empty() {
        return None;
    }
    comment
        .split(|char: char| {
            matches!(
                char,
                ' ' | '\t' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        })
        .map(|token| token.trim_matches(['.', ':']))
        .find(|token| parse_version_like(token))
}

fn parse_version_like(value: &str) -> bool {
    let rest = value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value);
    !rest.is_empty()
        && rest.as_bytes()[0].is_ascii_digit()
        && rest
            .chars()
            .all(|char| char == '.' || char.is_ascii_digit())
}

fn is_composite_action(path: &str) -> bool {
    path.ends_with("action.yml") || path.ends_with("action.yaml")
}

#[cfg(test)]
mod tests {
    use yamlpath::route;

    use super::*;

    #[test]
    fn scalar_at_route_removes_quotes_and_preserves_ref_span() {
        let source = "uses: \"actions/setup-node@v4\"\n";
        let document = parse_document(source).unwrap();
        let value = scalar_at_route(&document, &route!("uses"))
            .unwrap()
            .unwrap();
        assert_eq!("actions/setup-node@v4", value.text);
        assert_eq!(
            "actions/setup-node@v4",
            &source[value.start_byte..value.end_byte]
        );
    }

    #[test]
    fn scalar_at_route_finds_trailing_comment() {
        let source = "uses: \"actions/setup-node@v4#literal\" # v4.2.0\n";
        let document = parse_document(source).unwrap();
        let value = scalar_at_route(&document, &route!("uses"))
            .unwrap()
            .unwrap();
        assert_eq!("v4.2.0", value.trailing_comment);
    }

    #[test]
    fn collects_workflow_step_and_reusable_workflow_references() {
        let source = concat!(
            "jobs:\n",
            "  build:\n",
            "    uses: luxass/shared-workflows/.github/workflows/ci.yml@v1\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4 # v4.1.0\n",
            "      - uses: ./local-action\n",
        );
        let found = collect_references(".github/workflows/ci.yml", source).unwrap();
        assert_eq!(2, found.len());
        assert_eq!(ReferenceKind::WorkflowJob, found[0].kind);
        assert_eq!(ReferenceKind::WorkflowStep, found[1].kind);
        assert_eq!(
            "luxass/shared-workflows/.github/workflows/ci.yml",
            found[0].name.display()
        );
        assert_eq!("build", found[0].scope);
        assert_eq!("v1", found[0].current_ref);
        assert_eq!("actions/checkout", found[1].name.display());
        assert_eq!("v4.1.0", found[1].version_hint);
    }

    #[test]
    fn parses_quoted_uses_span() {
        let source = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: \"actions/setup-node@v4\"\n",
        );
        let found = collect_references(".github/workflows/ci.yml", source).unwrap();
        assert_eq!(1, found.len());
        assert_eq!("v4", found[0].current_ref);
        assert_eq!(
            "v4",
            &source[found[0].source.ref_span.start..found[0].source.ref_span.end]
        );
    }

    #[test]
    fn ignores_local_and_docker_references() {
        let source = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: ./local-action\n",
            "      - uses: ../shared-action\n",
            "      - uses: docker://alpine:3.20\n",
        );

        let found = collect_references(".github/workflows/ci.yml", source).unwrap();

        assert!(found.is_empty());
    }

    #[test]
    fn collects_composite_action_steps() {
        let source = concat!(
            "name: Example\n",
            "runs:\n",
            "  using: composite\n",
            "  steps:\n",
            "    - uses: actions/setup-node@v4 # v4.0.0\n",
        );

        let found = collect_references("action.yml", source).unwrap();

        assert_eq!(1, found.len());
        assert_eq!(ReferenceKind::CompositeStep, found[0].kind);
        assert_eq!("composite", found[0].scope);
        assert_eq!("actions/setup-node", found[0].name.display());
        assert_eq!("v4.0.0", found[0].version_hint);
    }
}
