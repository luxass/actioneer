use std::process::ExitCode;

use owo_colors::OwoColorize;

use crate::actions::{ActionUpdate, UpdateNote, is_likely_sha, resolve};
use crate::cli::{GlobalArgs, ScanArgs};
use crate::cmd::{
    apply_filters, default_inputs, describe_sha_mismatch, discover_actions, fetch_tags_reporting,
    plural, resolve_config,
};
use crate::github::GitHubClient;
use crate::terminal::display::{Printer, print_json, short_sha, update_file_count};
use crate::terminal::prompt;
use crate::workflows::{PatchError, apply_patches};

pub fn run(global: GlobalArgs, args: ScanArgs, gh: GitHubClient) -> ExitCode {
    let printer = Printer::new(global.mode);
    let inputs = default_inputs(args.inputs.clone(), args.recursive);

    let actions = match discover_actions(&printer, global.mode, &inputs, args.recursive) {
        Ok(actions) => actions,
        Err(code) => return code,
    };

    let tags = match fetch_tags_reporting(&printer, &actions, &gh) {
        Ok(tags) => tags,
        Err(code) => return code,
    };

    let updates = apply_filters(
        resolve(&actions, &tags, &resolve_config(&global, &args)),
        &args.filters,
    );

    if global.mode.is_json() {
        print_json(&updates);
        return ExitCode::SUCCESS;
    }

    printer.info(&format!(
        "Resolved {} available update{} across {} workflow file{}.",
        updates.len().to_string().yellow(),
        plural(updates.len()),
        update_file_count(&updates).to_string().yellow(),
        plural(update_file_count(&updates)),
    ));

    let mismatch_count = updates.iter().filter(|a| a.sha_mismatch).count();
    if mismatch_count > 0 {
        printer.warn(&format!(
            "{} pinned SHA{} do not match their version comments.",
            mismatch_count.to_string().yellow(),
            plural(mismatch_count),
        ));
        for a in updates.iter().filter(|a| a.sha_mismatch) {
            printer.warn(&describe_sha_mismatch(a));
        }
    }

    let branch_count = updates.iter().filter(|a| a.is_branch).count();
    if branch_count > 0 {
        printer.warn(&format!(
            "{} action reference{} use mutable branch refs (e.g. @main, @master). These are insecure and should be pinned to a version tag or SHA.",
            branch_count.to_string().yellow(),
            plural(branch_count),
        ));
    }

    if global.dry_run {
        printer.info(&format!(
            "Preview: {} available update{}.",
            updates.len().to_string().yellow(),
            plural(updates.len()),
        ));
        let selected: Vec<_> = (0..updates.len()).collect();
        print_update_list(&printer, &updates, &selected);
        return ExitCode::SUCCESS;
    }

    if updates.is_empty() {
        printer.info("Everything is already up to date.");
        return ExitCode::SUCCESS;
    }

    let selected = if args.yes {
        (0..updates.len()).collect()
    } else {
        match prompt::select(&updates) {
            Ok(s) => s,
            Err(prompt::Error::NotATerminal) => {
                printer.error("Interactive selection is not available in this terminal.");
                printer.info(&format!(
                    "Use {}, {}, or {}.",
                    "--yes".cyan(),
                    "--dry-run".cyan(),
                    "--mode json".cyan()
                ));
                return ExitCode::FAILURE;
            }
            Err(prompt::Error::Canceled) => {
                printer.warn("Selection canceled.");
                return ExitCode::SUCCESS;
            }
            Err(prompt::Error::Interrupted) => {
                printer.warn("Selection interrupted.");
                return ExitCode::FAILURE;
            }
            Err(e) => {
                printer.error(&format!("Prompt error: {e}"));
                return ExitCode::FAILURE;
            }
        }
    };

    if selected.is_empty() {
        printer.info("No updates selected. No files were changed.");
        return ExitCode::SUCCESS;
    }

    let selected_files = selected_file_count(&updates, &selected);
    printer.info(&format!(
        "Applying {} selected update{} across {} file{}:",
        selected.len().to_string().yellow(),
        plural(selected.len()),
        selected_files.to_string().yellow(),
        plural(selected_files),
    ));
    print_update_list(&printer, &updates, &selected);

    match apply_patches(&updates, &selected) {
        Ok(applied) => {
            let files = selected_file_count(&updates, &selected);
            printer.info(&format!(
                "Updated {} workflow reference{} across {} file{}.",
                applied.to_string().yellow(),
                plural(applied),
                files.to_string().yellow(),
                plural(files),
            ));
            ExitCode::SUCCESS
        }
        Err(err) => {
            printer.error(&format!("Could not write selected updates: {err}."));
            match &err {
                PatchError::UpdateTargetNotFound => {
                    printer.info("Re-run actioneer so it can scan the current file contents.");
                }
                _ => {
                    printer.info("Some files may already have been written. Review your working tree before retrying.");
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn print_update_list(printer: &Printer, updates: &[ActionUpdate], selected: &[usize]) {
    let mut current_file = None;
    for &idx in selected {
        let update = &updates[idx];
        if current_file != Some(update.action.file.as_str()) {
            current_file = Some(update.action.file.as_str());
            printer.info(&update.action.file.cyan().to_string());
        }

        let mut line = format!(
            "  L{} {}: {}",
            update.action.line.to_string().bright_black(),
            update.action_name().bold(),
            format_update_change(update)
        );
        append_notes(&mut line, update);
        printer.info(&line);
    }
}

fn selected_file_count(updates: &[ActionUpdate], selected: &[usize]) -> usize {
    updates
        .iter()
        .enumerate()
        .filter_map(|(i, a)| selected.contains(&i).then_some(a.action.file.as_str()))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn format_update_change(update: &ActionUpdate) -> String {
    let version_change = if update.is_major || update.is_branch {
        update.version_label().red().to_string()
    } else {
        update.version_label().green().to_string()
    };
    let current_label = update
        .action
        .version_comment
        .as_deref()
        .unwrap_or(&update.action.current_ref);

    if update.action.current_ref == current_label && !update.ref_differs_from_version() {
        return version_change;
    }

    format!(
        "{} ({} -> {})",
        version_change,
        format_ref_detail(&update.action.current_ref).bright_black(),
        format_ref_detail(&update.new_ref).bright_black()
    )
}

fn format_ref_detail(value: &str) -> &str {
    if is_likely_sha(value) {
        short_sha(value)
    } else {
        value
    }
}

fn append_notes(line: &mut String, update: &ActionUpdate) {
    for note in update.notes() {
        match note {
            UpdateNote::ShaMismatch => {
                line.push_str(&format!(" {}", "(SHA/comment mismatch)".red()));
            }
            UpdateNote::MutableBranch => {
                line.push_str(&format!(" {}", "(unpinned branch ref)".yellow()));
            }
            UpdateNote::MajorUpdate => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{ActionReference, fixtures};
    use crate::terminal::display::strip_ansi;

    #[test]
    fn update_change_prefers_version_comment_and_shortens_shas() {
        let update = action_update(
            "de0fac2e4500dabe0009e67214ff5f5447ce83dd",
            Some("v6.0.2"),
            "df4cb1c069e1874edd31b4311f1884172cec0e10",
            "v6.0.3",
        );

        assert_eq!(
            "v6.0.2 -> v6.0.3 (de0fac2e4500 -> df4cb1c069e1)",
            strip_ansi(&format_update_change(&update))
        );
    }

    #[test]
    fn update_change_omits_ref_detail_for_tag_updates() {
        let update = action_update("v4.1.0", None, "v4.2.0", "v4.2.0");

        assert_eq!(
            "v4.1.0 -> v4.2.0",
            strip_ansi(&format_update_change(&update))
        );
    }

    fn action_update(
        current_ref: &str,
        version_comment: Option<&str>,
        new_ref: &str,
        new_version: &str,
    ) -> ActionUpdate {
        ActionUpdate {
            new_ref: new_ref.into(),
            new_version: new_version.into(),
            expected_sha: new_ref.into(),
            ..fixtures::update(ActionReference {
                current_ref: current_ref.into(),
                version_comment: version_comment.map(str::to_string),
                file: ".github/workflows/ci.yaml".into(),
                ..fixtures::reference()
            })
        }
    }
}
