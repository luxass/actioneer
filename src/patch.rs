use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::{config::PinStyle, update::Candidate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchEdit {
    pub file: PathBuf,
    pub line: usize,
    pub old_text: String,
    pub new_text: String,
}

pub fn update_patch_edits(candidates: &[Candidate], selected_indexes: &[usize]) -> Vec<PatchEdit> {
    selected_indexes
        .iter()
        .filter_map(|&index| candidates.get(index))
        .map(|candidate| PatchEdit {
            file: candidate.action.file.clone(),
            line: candidate.action.line,
            old_text: current_uses_text(candidate),
            new_text: target_uses_text(candidate),
        })
        .collect()
}

pub fn apply_patch_edits(edits: &[PatchEdit]) -> Result<(), String> {
    let mut edits_by_file = BTreeMap::<PathBuf, Vec<&PatchEdit>>::new();
    for edit in edits {
        edits_by_file
            .entry(edit.file.clone())
            .or_default()
            .push(edit);
    }

    for (file, edits) in edits_by_file {
        let contents = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {} for patching: {error}", file.display()))?;
        let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
        let had_trailing_newline = contents.ends_with('\n');

        for edit in edits {
            let line_index = edit.line.checked_sub(1).ok_or_else(|| {
                format!("invalid patch line {} in {}", edit.line, edit.file.display())
            })?;
            let line = lines.get_mut(line_index).ok_or_else(|| {
                format!(
                    "cannot patch {}:{} because the line no longer exists",
                    edit.file.display(),
                    edit.line
                )
            })?;

            if !line.contains(&edit.old_text) {
                return Err(format!(
                    "cannot patch {}:{} because {:?} is no longer present",
                    edit.file.display(),
                    edit.line,
                    edit.old_text
                ));
            }

            *line = line.replacen(&edit.old_text, &edit.new_text, 1);
        }

        let mut patched = lines.join("\n");
        if had_trailing_newline {
            patched.push('\n');
        }
        fs::write(&file, patched)
            .map_err(|error| format!("failed to write patched file {}: {error}", file.display()))?;
    }

    Ok(())
}

fn current_uses_text(candidate: &Candidate) -> String {
    format!("{}@{}", action_name(candidate), candidate.action.ref_name)
}

fn target_uses_text(candidate: &Candidate) -> String {
    let target = format!("{}@{}", action_name(candidate), candidate.target_ref);
    if candidate.pin == PinStyle::Sha && candidate.target_ref != candidate.version {
        format!("{target} # {}", candidate.version)
    } else {
        target
    }
}

fn action_name(candidate: &Candidate) -> String {
    if candidate.action.path.is_empty() {
        candidate.action.repo.clone()
    } else {
        format!("{}/{}", candidate.action.repo, candidate.action.path)
    }
}
