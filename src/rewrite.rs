use std::fs;

use crate::model::Action;

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("update target not found in source file")]
    UpdateTargetNotFound,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn apply(actions: &[Action], selected: &[usize]) -> Result<usize, RewriteError> {
    let selected_actions: Vec<&Action> = selected.iter().filter_map(|&i| actions.get(i)).collect();

    let mut remaining: &[&Action] = &selected_actions;
    let mut total_applied = 0;

    while let Some(first) = remaining.first() {
        let file = &first.file;
        let count = remaining.iter().take_while(|a| a.file == *file).count();
        let (file_actions, rest) = remaining.split_at(count);

        let original = fs::read_to_string(file)?;
        let rewritten = rewrite_text(&original, file_actions)?;
        fs::write(file, rewritten)?;
        total_applied += file_actions.len();
        remaining = rest;
    }

    Ok(total_applied)
}

fn rewrite_text(contents: &str, actions: &[&Action]) -> Result<String, RewriteError> {
    let mut actions: Vec<_> = actions.to_vec();
    actions.sort_by_key(|a| a.ref_start);

    for action in &actions {
        if action.ref_start > action.ref_end
            || action.ref_end > contents.len()
            || contents[action.ref_start..action.ref_end] != action.current_ref
        {
            return Err(RewriteError::UpdateTargetNotFound);
        }
    }

    let mut output = String::with_capacity(contents.len());
    let mut cursor = 0;

    for action in &actions {
        output.push_str(&contents[cursor..action.ref_start]);
        output.push_str(&action.new_ref);
        cursor = action.ref_end;

        if !should_write_comment(action) {
            continue;
        }

        let line_end = contents[cursor..]
            .find('\n')
            .map(|rel| {
                let abs = cursor + rel;
                if abs > 0 && contents.as_bytes()[abs - 1] == b'\r' {
                    abs - 1
                } else {
                    abs
                }
            })
            .unwrap_or(contents.len());

        let comment_start = find_comment_start(contents, cursor);
        let comment_pos = comment_start
            .map(|cs| {
                let mut s = cs;
                while s > cursor && matches!(contents.as_bytes()[s - 1], b' ' | b'\t') {
                    s -= 1;
                }
                s
            })
            .unwrap_or(line_end);

        output.push_str(&contents[cursor..comment_pos]);
        output.push_str(&format!(" # {}", action.new_version));
        cursor = line_end;
    }

    output.push_str(&contents[cursor..]);
    Ok(output)
}

fn should_write_comment(action: &Action) -> bool {
    !action.new_version.is_empty()
        && (action.new_ref != action.new_version
            || action.version_comment.is_some()
            || action.sha_mismatch)
}

fn find_comment_start(contents: &str, offset: usize) -> Option<usize> {
    let line_start = contents[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = contents[offset..]
        .find('\n')
        .map(|rel| offset + rel)
        .unwrap_or(contents.len());

    let mut active_quote: Option<char> = None;
    for (rel, ch) in contents[line_start..line_end].char_indices() {
        let idx = line_start + rel;
        if let Some(q) = active_quote {
            if ch == q {
                active_quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            active_quote = Some(ch);
            continue;
        }
        if ch == '#' {
            return Some(idx);
        }
    }
    None
}
