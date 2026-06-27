//! YAML deserialization and reference extraction.
//!
//! This module owns the internal `Raw*` types used only for serde deserialization
//! and the line-number scan. None of the `Raw*` types are part of the public API.

use indexmap::IndexMap;
use serde::Deserialize;

use super::{reference::parse_uses, ActionReference, ParseError, WorkflowDocument};

/// Parse a GitHub Actions workflow from its raw YAML content.
///
/// Returns a [`WorkflowDocument`] with all `uses:` references in document order.
/// Both job-level (`jobs.<id>.uses`) and step-level (`jobs.<id>.steps[*].uses`)
/// references are included.
///
/// # Errors
///
/// Returns [`ParseError::Yaml`] if the content is not valid YAML or does not
/// match the expected workflow shape. Note that unknown YAML keys are silently
/// ignored, so only structural mismatches (wrong types, etc.) cause errors.
pub fn parse_workflow(content: &str) -> Result<WorkflowDocument, ParseError> {
    let raw: RawWorkflow = serde_yaml::from_str(content)?;
    let mut references = Vec::new();

    for (job_id, job) in &raw.jobs {
        // Job-level reusable-workflow call.
        if let Some(raw_uses) = &job.uses {
            let p = parse_uses(raw_uses);
            references.push(ActionReference {
                raw: raw_uses.clone(),
                kind: p.kind,
                pin_kind: p.pin_kind,
                owner: p.owner,
                repo: p.repo,
                subpath: p.subpath,
                git_ref: p.git_ref,
                step_name: None,
                job_id: job_id.clone(),
                job_name: job.name.clone(),
                step_index: None,
                line: None,
                line_comment: None,
            });
        }

        for (idx, step) in job.steps.iter().enumerate() {
            if let Some(raw_uses) = &step.uses {
                let p = parse_uses(raw_uses);
                references.push(ActionReference {
                    raw: raw_uses.clone(),
                    kind: p.kind,
                    pin_kind: p.pin_kind,
                    owner: p.owner,
                    repo: p.repo,
                    subpath: p.subpath,
                    git_ref: p.git_ref,
                    step_name: step.name.clone(),
                    job_id: job_id.clone(),
                    job_name: job.name.clone(),
                    step_index: Some(idx),
                    line: None,
                    line_comment: None,
                });
            }
        }
    }

    assign_line_numbers(content, &mut references);

    Ok(WorkflowDocument {
        name: raw.name,
        references,
    })
}

/// Scan `content` line by line to assign 1-based line numbers and extract trailing
/// comments from each reference's `uses:` line.
///
/// The scan cursor advances monotonically: each reference consumes the next
/// matching `uses:` line. This works correctly as long as references are passed
/// in document order (which [`parse_workflow`] guarantees via [`IndexMap`] job
/// order and sequential step iteration).
///
/// Limitation: if the same `uses:` value appears multiple times within a single
/// job and out-of-order relative to the source, line assignment may be off by
/// one occurrence. See `docs/engine.md` § "Line tracking".
fn assign_line_numbers(content: &str, refs: &mut [ActionReference]) {
    let lines: Vec<&str> = content.lines().collect();
    let mut cursor = 0usize;

    for reference in refs.iter_mut() {
        while cursor < lines.len() {
            let trimmed = lines[cursor].trim();
            // A `uses:` key can appear as `uses: val` (inside a mapping block)
            // or as `- uses: val` (first key on an inline sequence entry).
            let uses_part = trimmed
                .strip_prefix("- ")
                .map(str::trim_start)
                .unwrap_or(trimmed);
            if let Some(rest) = uses_part.strip_prefix("uses:") {
                let (val, comment) = extract_uses_comment(rest);
                if val == reference.raw {
                    reference.line = Some(cursor as u32 + 1);
                    reference.line_comment = comment;
                    cursor += 1;
                    break;
                }
            }
            cursor += 1;
        }
    }
}

/// Split the value portion of a `uses:` line into the action ref and an optional
/// trailing comment.
///
/// `rest` is everything after the `uses:` key (e.g. `  actions/checkout@v4 # v4.2.0`).
///
/// Returns `(value, comment)` where:
/// - `value` is the trimmed action ref with quotes stripped.
/// - `comment` is the trimmed text after `#`, or `None` if absent or empty.
///
/// Handles both unquoted values and values wrapped in `"..."` or `'...'`.
fn extract_uses_comment(rest: &str) -> (String, Option<String>) {
    let trimmed = rest.trim();

    // Quoted value: find the closing quote first so that a `#` inside the
    // quoted string is not mistaken for a comment delimiter.
    if let Some(inner) = trimmed.strip_prefix('"') {
        if let Some(close) = inner.find('"') {
            let value = &inner[..close];
            let remainder = &inner[close + 1..];
            return (value.to_string(), comment_from_remainder(remainder));
        }
    } else if let Some(inner) = trimmed.strip_prefix('\'')
        && let Some(close) = inner.find('\'')
    {
        let value = &inner[..close];
        let remainder = &inner[close + 1..];
        return (value.to_string(), comment_from_remainder(remainder));
    }

    // Unquoted value: split at the first `#`.
    if let Some(hash) = trimmed.find('#') {
        let value = trimmed[..hash].trim_end();
        let comment_text = trimmed[hash + 1..].trim();
        return (
            value.to_string(),
            if comment_text.is_empty() {
                None
            } else {
                Some(comment_text.to_string())
            },
        );
    }

    (trimmed.to_string(), None)
}

/// Extract an optional comment from the text that follows the closing quote of a
/// YAML-quoted `uses:` value.
fn comment_from_remainder(remainder: &str) -> Option<String> {
    let hash = remainder.find('#')?;
    let text = remainder[hash + 1..].trim();
    if text.is_empty() { None } else { Some(text.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_unquoted_no_comment() {
        let (val, comment) = extract_uses_comment("  actions/checkout@v4");
        assert_eq!(val, "actions/checkout@v4");
        assert!(comment.is_none());
    }

    #[test]
    fn extract_unquoted_with_comment() {
        let (val, comment) = extract_uses_comment("  actions/checkout@v4 # v4.2.0");
        assert_eq!(val, "actions/checkout@v4");
        assert_eq!(comment.as_deref(), Some("v4.2.0"));
    }

    #[test]
    fn extract_unquoted_empty_comment() {
        let (val, comment) = extract_uses_comment("  actions/checkout@v4 #");
        assert_eq!(val, "actions/checkout@v4");
        assert!(comment.is_none());
    }

    #[test]
    fn extract_double_quoted_with_comment() {
        let (val, comment) = extract_uses_comment(r#"  "actions/checkout@v4" # v4.2.0"#);
        assert_eq!(val, "actions/checkout@v4");
        assert_eq!(comment.as_deref(), Some("v4.2.0"));
    }

    #[test]
    fn extract_single_quoted_with_comment() {
        let (val, comment) = extract_uses_comment("  'actions/checkout@v4' # v4.2.0");
        assert_eq!(val, "actions/checkout@v4");
        assert_eq!(comment.as_deref(), Some("v4.2.0"));
    }

    #[test]
    fn extract_sha_with_tag_comment() {
        let (val, comment) = extract_uses_comment(
            "  actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675 # v4.2.0",
        );
        assert_eq!(val, "actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675");
        assert_eq!(comment.as_deref(), Some("v4.2.0"));
    }
}

#[derive(Debug, Deserialize)]
struct RawWorkflow {
    name: Option<String>,
    #[serde(default)]
    jobs: IndexMap<String, RawJob>,
}

#[derive(Debug, Deserialize)]
struct RawJob {
    name: Option<String>,
    /// Job-level reusable-workflow call (`jobs.<id>.uses`).
    uses: Option<String>,
    #[serde(default)]
    steps: Vec<RawStep>,
}

#[derive(Debug, Deserialize)]
struct RawStep {
    name: Option<String>,
    uses: Option<String>,
}
