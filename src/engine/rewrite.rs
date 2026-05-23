use std::collections::{BTreeMap, HashSet};
use std::fs;

use thiserror::Error;

use crate::model::ResolvedUpdate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("invalid edit range")]
    InvalidEditRange,
    #[error("overlapping edits")]
    OverlappingEdits,
}

#[derive(Debug, Error)]
pub enum RewriteError {
    #[error("update target not found")]
    UpdateTargetNotFound,
    #[error(transparent)]
    Apply(#[from] ApplyError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct FileRewrite {
    pub contents: String,
    pub applied: usize,
}

pub fn apply_selected_updates(
    updates: &[ResolvedUpdate],
    selected: &[usize],
) -> Result<usize, RewriteError> {
    let mut applied = 0;
    let mut written_files = HashSet::new();

    for (file, file_updates) in collect_updates_by_file(updates, selected) {
        if !written_files.insert(file.clone()) {
            continue;
        }

        let original = fs::read_to_string(&file)?;
        let rewrite = apply_updates_to_text(&original, &file, &file_updates)?;
        if rewrite.applied == 0 {
            continue;
        }

        fs::write(&file, rewrite.contents)?;
        applied += rewrite.applied;
    }

    Ok(applied)
}

pub fn apply_updates_to_text(
    contents: &str,
    file: &str,
    updates: &[&ResolvedUpdate],
) -> Result<FileRewrite, RewriteError> {
    let ordered_updates = validate_and_sort_updates(contents, file, updates)?;
    let edits = build_text_edits(contents, &ordered_updates);
    let rewritten = apply_text_edits(contents, &edits)?;

    Ok(FileRewrite {
        contents: rewritten,
        applied: ordered_updates.len(),
    })
}

fn collect_updates_by_file<'a>(
    updates: &'a [ResolvedUpdate],
    selected: &[usize],
) -> BTreeMap<String, Vec<&'a ResolvedUpdate>> {
    let mut grouped = BTreeMap::new();

    for &index in selected {
        if let Some(update) = updates.get(index) {
            grouped
                .entry(update.file().to_string())
                .or_insert_with(Vec::new)
                .push(update);
        }
    }

    grouped
}

fn validate_and_sort_updates<'a>(
    contents: &str,
    file: &str,
    updates: &[&'a ResolvedUpdate],
) -> Result<Vec<&'a ResolvedUpdate>, RewriteError> {
    let mut ordered = updates.to_vec();
    ordered.sort_by_key(|update| update.ref_start());

    for update in &ordered {
        if update.file() != file {
            continue;
        }
        if update.ref_start() > update.ref_end() || update.ref_end() > contents.len() {
            return Err(RewriteError::UpdateTargetNotFound);
        }
        if contents[update.ref_start()..update.ref_end()] != update.current {
            return Err(RewriteError::UpdateTargetNotFound);
        }
    }

    Ok(ordered)
}

fn build_text_edits(contents: &str, updates: &[&ResolvedUpdate]) -> Vec<TextEdit> {
    let mut edits = Vec::with_capacity(updates.len() * 2);

    for update in updates {
        edits.push(TextEdit {
            start: update.ref_start(),
            end: update.ref_end(),
            replacement: update.next_ref().to_string(),
        });

        if !update.should_write_version_comment() {
            continue;
        }

        let line_end = line_end_offset(contents, update.ref_end());
        let comment_start = comment_start_offset(contents, update.ref_end());
        let replacement_start = comment_start
            .map(|start| trim_comment_padding(contents, update.ref_end(), start))
            .unwrap_or(line_end);

        edits.push(TextEdit {
            start: replacement_start,
            end: line_end,
            replacement: format!(" # {}", update.display_target()),
        });
    }

    edits
}

fn line_end_offset(contents: &str, offset: usize) -> usize {
    contents[offset..]
        .find('\n')
        .map(|relative| {
            let abs = offset + relative;
            if abs > 0 && contents.as_bytes()[abs - 1] == b'\r' {
                abs - 1
            } else {
                abs
            }
        })
        .unwrap_or(contents.len())
}

fn comment_start_offset(contents: &str, offset: usize) -> Option<usize> {
    let line_start = contents[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line_end = line_end_offset(contents, offset);
    let mut active_quote = None;

    for (relative, ch) in contents[line_start..line_end].char_indices() {
        let index = line_start + relative;
        if let Some(quote) = active_quote {
            if ch == quote {
                active_quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'') {
            active_quote = Some(ch);
            continue;
        }
        if ch == '#' {
            return Some(index);
        }
    }

    None
}

fn trim_comment_padding(contents: &str, ref_end: usize, comment_start: usize) -> usize {
    let mut start = comment_start;
    while start > ref_end && matches!(contents.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    start
}

fn apply_text_edits(contents: &str, edits: &[TextEdit]) -> Result<String, ApplyError> {
    if edits.is_empty() {
        return Ok(contents.to_string());
    }

    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| edit.start);

    let mut output = String::with_capacity(contents.len());
    let mut cursor = 0;

    for edit in ordered {
        if edit.start > edit.end || edit.end > contents.len() {
            return Err(ApplyError::InvalidEditRange);
        }
        if edit.start < cursor {
            return Err(ApplyError::OverlappingEdits);
        }

        output.push_str(&contents[cursor..edit.start]);
        output.push_str(&edit.replacement);
        cursor = edit.end;
    }

    output.push_str(&contents[cursor..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use crate::model::{UpdateSource, UpdateTarget, ValidationState};

    use super::*;

    #[test]
    fn applies_sha_refresh_and_comment_refresh() {
        let input = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@oldsha # v4.1.0\n",
            "      - uses: actions/setup-node@v3\n",
        );
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldsha",
            ValidationState::new("", "v4.1.0", false),
            UpdateTarget::new("newsha", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(1, rewrite.applied);
        assert_eq!(
            concat!(
                "jobs:\n",
                "  build:\n",
                "    steps:\n",
                "      - uses: actions/checkout@newsha # v4.2.0\n",
                "      - uses: actions/setup-node@v3\n",
            ),
            rewrite.contents
        );
    }

    #[test]
    fn preserves_quotes_around_the_uses_value() {
        let input = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: \"actions/setup-node@oldsha\" # v6.2.0\n",
        );
        let current = "oldsha";
        let update = ResolvedUpdate::new(
            "actions/setup-node",
            "build",
            current,
            ValidationState::new("", "v6.2.0", false),
            UpdateTarget::new("newsha", "v6.4.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find(current).unwrap(),
                input.find(current).unwrap() + current.len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(
            concat!(
                "jobs:\n",
                "  build:\n",
                "    steps:\n",
                "      - uses: \"actions/setup-node@newsha\" # v6.4.0\n",
            ),
            rewrite.contents
        );
    }

    #[test]
    fn applies_multiple_updates_in_one_file() {
        let input = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@oldcheckout # v4.1.0\n",
            "      - uses: actions/setup-node@oldnode # v6.2.0\n",
        );
        let checkout = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldcheckout",
            ValidationState::new("", "v4.1.0", false),
            UpdateTarget::new("newcheckout", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldcheckout").unwrap(),
                input.find("oldcheckout").unwrap() + "oldcheckout".len(),
            ),
            false,
        );
        let setup_node = ResolvedUpdate::new(
            "actions/setup-node",
            "build",
            "oldnode",
            ValidationState::new("", "v6.2.0", false),
            UpdateTarget::new("newnode", "v6.4.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                5,
                input.find("oldnode").unwrap(),
                input.find("oldnode").unwrap() + "oldnode".len(),
            ),
            false,
        );

        let rewrite =
            apply_updates_to_text(input, ".github/workflows/ci.yml", &[&checkout, &setup_node])
                .unwrap();

        assert_eq!(
            concat!(
                "jobs:\n",
                "  build:\n",
                "    steps:\n",
                "      - uses: actions/checkout@newcheckout # v4.2.0\n",
                "      - uses: actions/setup-node@newnode # v6.4.0\n",
            ),
            rewrite.contents
        );
    }

    #[test]
    fn fails_when_source_text_no_longer_matches_the_saved_span() {
        let input = concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@oldsha # v4.1.0\n",
        );
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "wrongsha",
            ValidationState::new("", "v4.1.0", false),
            UpdateTarget::new("newsha", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let err = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap_err();

        assert!(matches!(err, RewriteError::UpdateTargetNotFound));
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let input = "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha # v4.1.0\r\n      - uses: actions/setup-node@v3\r\n";
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldsha",
            ValidationState::new("", "v4.1.0", false),
            UpdateTarget::new("newsha", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(1, rewrite.applied);
        assert_eq!("jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@newsha # v4.2.0\r\n      - uses: actions/setup-node@v3\r\n", rewrite.contents);
    }

    #[test]
    fn preserves_crlf_without_existing_comment() {
        let input = "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha\r\n";
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldsha",
            ValidationState::new("", "", false),
            UpdateTarget::new("newsha", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(1, rewrite.applied);
        assert_eq!("jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@newsha # v4.2.0\r\n", rewrite.contents);
    }

    #[test]
    fn preserves_crlf_when_no_comment_written() {
        let input = "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha\r\n";
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldsha",
            ValidationState::new("", "", false),
            UpdateTarget::new("v4.2.0", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(1, rewrite.applied);
        assert_eq!("jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@v4.2.0\r\n", rewrite.contents);
    }

    #[test]
    fn preserves_crlf_with_version_comment_write() {
        let input = "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha\r\n";
        let update = ResolvedUpdate::new(
            "actions/checkout",
            "build",
            "oldsha",
            ValidationState::new("abc123", "v4.1.0", true),
            UpdateTarget::new("newsha", "v4.2.0", false),
            UpdateSource::new(
                ".github/workflows/ci.yml",
                4,
                input.find("oldsha").unwrap(),
                input.find("oldsha").unwrap() + "oldsha".len(),
            ),
            false,
        );

        let rewrite = apply_updates_to_text(input, ".github/workflows/ci.yml", &[&update]).unwrap();

        assert_eq!(1, rewrite.applied);
        assert_eq!("jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@newsha # v4.2.0\r\n", rewrite.contents);
    }
}
