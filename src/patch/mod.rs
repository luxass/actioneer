use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::{config::PinStyle, update::UpdatePlan};

pub fn apply_update_plan(plan: &mut UpdatePlan) -> Result<(), String> {
    let mut edits_by_file = BTreeMap::<PathBuf, Vec<usize>>::new();
    for (index, candidate) in plan.candidates.iter().enumerate() {
        if candidate.selected {
            edits_by_file
                .entry(PathBuf::from(&candidate.file))
                .or_default()
                .push(index);
        }
    }

    for (file, candidate_indexes) in edits_by_file {
        let contents = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {} for patching: {error}", file.display()))?;
        let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
        let had_trailing_newline = contents.ends_with('\n');

        for candidate_index in candidate_indexes {
            let candidate = &plan.candidates[candidate_index];
            let line_index = candidate.line.checked_sub(1).ok_or_else(|| {
                format!("invalid patch line {} in {}", candidate.line, candidate.file)
            })?;
            let line = lines.get_mut(line_index).ok_or_else(|| {
                format!(
                    "cannot patch {}:{} because the line no longer exists",
                    candidate.file, candidate.line
                )
            })?;

            let old_uses = current_uses_text(candidate);
            let new_uses = target_uses_text(candidate);
            if !line.contains(&old_uses) {
                return Err(format!(
                    "cannot patch {}:{} because {old_uses:?} is no longer present",
                    candidate.file, candidate.line
                ));
            }

            *line = line.replacen(&old_uses, &new_uses, 1);
            plan.candidates[candidate_index].applied = true;
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

fn current_uses_text(candidate: &crate::update::UpdateCandidate) -> String {
    format!("{}@{}", action_name(candidate), candidate.action.current_ref)
}

fn target_uses_text(candidate: &crate::update::UpdateCandidate) -> String {
    let target = format!("{}@{}", action_name(candidate), candidate.target.ref_name);
    if candidate.target.pin == PinStyle::Sha && candidate.target.ref_name != candidate.target.version {
        format!("{target} # {}", candidate.target.version)
    } else {
        target
    }
}

fn action_name(candidate: &crate::update::UpdateCandidate) -> String {
    if candidate.action.path.is_empty() {
        candidate.action.repo.clone()
    } else {
        format!("{}/{}", candidate.action.repo, candidate.action.path)
    }
}
