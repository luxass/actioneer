use std::collections::{HashMap, HashSet};
use std::process::ExitCode;

use owo_colors::OwoColorize;

use crate::cli::{GlobalArgs, ScanArgs};
use crate::cmd::default_inputs;
use crate::display::{Printer, print_json, short_sha, update_file_count};
use crate::github::{Error as GitHubError, GitHubClient};
use crate::model::ResolveConfig;
use crate::{resolve, scan};

pub fn run(global: GlobalArgs, args: ScanArgs, gh: GitHubClient) -> anyhow::Result<ExitCode> {
    let printer = Printer::new(global.mode);
    let inputs = default_inputs(args.inputs, args.recursive);

    if inputs.len() == 1 {
        printer.info(&format!("Scanning workflows in {}", inputs[0].bold()));
    } else {
        printer.info(&format!(
            "Scanning {} input paths:",
            inputs.len().to_string().yellow()
        ));
        for input in &inputs {
            printer.debug(&format!("  {}", input.bright_black()));
        }
    }

    let mut actions = match scan::scan(&inputs, args.recursive) {
        Ok(a) => a,
        Err(err) => {
            printer.error(&format!("Scan failed: {err}"));
            return Ok(ExitCode::FAILURE);
        }
    };

    if actions.is_empty() {
        if global.mode.is_json() {
            print_json(&[]);
        } else {
            printer.warn("No action references found.");
            printer.info("Point actioneer at a workflow file or directory with `uses:` entries.");
        }
        return Ok(ExitCode::SUCCESS);
    }

    let repos: HashSet<(String, String)> = actions
        .iter()
        .map(|a| (a.owner.clone(), a.name.clone()))
        .collect();
    let mut tags: HashMap<(String, String), Vec<crate::model::Tag>> = HashMap::new();
    for (owner, name) in &repos {
        match gh.fetch_tags(owner, name) {
            Ok(repo_tags) => {
                tags.insert((owner.clone(), name.clone()), repo_tags);
            }
            Err(e) => {
                printer.error(&format!(
                    "GitHub lookup failed for {}/{}.",
                    owner.bold(),
                    name.bold()
                ));
                match &e {
                    GitHubError::HttpStatus(status) => {
                        printer.error(&format!(
                            "GitHub returned HTTP {}.",
                            status.to_string().yellow()
                        ));
                        let hint = match status {
                            401 => {
                                "Set GITHUB_TOKEN or run `gh auth login` so actioneer can authenticate GitHub requests."
                            }
                            403 => {
                                "This is usually a rate limit or access restriction. Set GITHUB_TOKEN or run `gh auth login` before retrying."
                            }
                            404 => "The repository was not found or is not publicly accessible.",
                            429 => "GitHub is rate limiting these requests.",
                            502..=504 => "GitHub appears temporarily unavailable.",
                            _ => {
                                "Retry later, or run with --dry-run/--mode json to inspect scanned references."
                            }
                        };
                        printer.info(hint);
                    }
                    GitHubError::Request(err) => {
                        printer.error(&format!("Request error: {}.", err.to_string().yellow()));
                        printer.info("Check network, DNS, proxy, and TLS settings. If you are unauthenticated, set GITHUB_TOKEN or run `gh auth login`.");
                    }
                }
                return Ok(ExitCode::FAILURE);
            }
        }
    }

    let resolve_config = ResolveConfig {
        excludes: global.excludes,
        skip_branches: args.skip_branches,
        mode: args.update,
        style: args.pin,
    };
    resolve::resolve(&mut actions, &tags, &resolve_config);
    actions.retain(|a| a.needs_update);

    if global.mode.is_json() {
        print_json(&actions);
        return Ok(ExitCode::SUCCESS);
    }

    printer.info(&format!(
        "Resolved {} available update{} across {} workflow file{}.",
        actions.len().to_string().yellow(),
        if actions.len() == 1 { "" } else { "s" },
        update_file_count(&actions).to_string().yellow(),
        if update_file_count(&actions) == 1 {
            ""
        } else {
            "s"
        },
    ));

    let mismatch_count = actions.iter().filter(|a| a.sha_mismatch).count();
    if mismatch_count > 0 {
        printer.warn(&format!(
            "{} pinned SHA{} do not match their version comments.",
            mismatch_count.to_string().yellow(),
            if mismatch_count == 1 { "" } else { "s" },
        ));
        for a in actions.iter().filter(|a| a.sha_mismatch) {
            let mut line = format!(
                "{} at {}:{} uses {}",
                a.action_name().bold(),
                a.file.cyan(),
                a.line,
                a.current_ref.red()
            );
            if let Some(vc) = &a.version_comment {
                line.push_str(&format!(" but says {}", vc.yellow()));
            }
            if !a.expected_sha.is_empty() {
                line.push_str(&format!(
                    "; expected {}",
                    short_sha(&a.expected_sha).green()
                ));
            }
            printer.warn(&format!("{line}."));
        }
    }

    let branch_count = actions.iter().filter(|a| a.is_branch).count();
    if branch_count > 0 {
        printer.warn(&format!(
            "{} action reference{} use mutable branch refs (e.g. @main, @master). These are insecure and should be pinned to a version tag or SHA.",
            branch_count.to_string().yellow(),
            if branch_count == 1 { "" } else { "s" },
        ));
    }

    if global.dry_run {
        printer.info(&format!(
            "Preview: {} available update{}.",
            actions.len().to_string().yellow(),
            if actions.len() == 1 { "" } else { "s" },
        ));
        for a in &actions {
            let target = if a.is_major || a.is_branch {
                a.new_ref.red().to_string()
            } else {
                a.new_ref.green().to_string()
            };
            let mut line = format!(
                "{}: {} -> {} ({}:{})",
                a.action_name().bold(),
                a.current_ref.yellow(),
                target,
                a.file.bright_black(),
                a.line
            );
            if a.new_ref != a.new_version {
                line.push_str(&format!(" [{}]", a.new_version.bright_black()));
            }
            if let Some(vc) = &a.version_comment {
                line.push_str(&format!(" #{}", vc.bright_black()));
            }
            if a.sha_mismatch {
                line.push_str(&format!(" {}", "(SHA/comment mismatch)".red()));
            }
            if a.is_branch {
                line.push_str(&format!(" {}", "(unpinned branch ref)".yellow()));
            }
            printer.info(&line);
        }
        return Ok(ExitCode::SUCCESS);
    }

    if actions.is_empty() {
        printer.info("Everything is already up to date.");
        return Ok(ExitCode::SUCCESS);
    }

    let selected = if args.yes {
        (0..actions.len()).collect()
    } else {
        match crate::prompt::select(&actions) {
            Ok(s) => s,
            Err(crate::prompt::Error::NotATerminal) => {
                printer.error("Interactive selection is not available in this terminal.");
                printer.info(&format!(
                    "Use {}, {}, or {}.",
                    "--yes".cyan(),
                    "--dry-run".cyan(),
                    "--mode json".cyan()
                ));
                return Ok(ExitCode::FAILURE);
            }
            Err(crate::prompt::Error::Canceled) => {
                printer.warn("Selection canceled.");
                return Ok(ExitCode::SUCCESS);
            }
            Err(crate::prompt::Error::Interrupted) => {
                printer.warn("Selection interrupted.");
                return Ok(ExitCode::FAILURE);
            }
            Err(e) => {
                printer.error(&format!("Prompt error: {e}"));
                return Ok(ExitCode::FAILURE);
            }
        }
    };

    if selected.is_empty() {
        printer.info("No updates selected. No files were changed.");
        return Ok(ExitCode::SUCCESS);
    }

    printer.info(&format!(
        "Applying {} selected update{}:",
        selected.len().to_string().yellow(),
        if selected.len() == 1 { "" } else { "s" },
    ));
    for &idx in &selected {
        let a = &actions[idx];
        let target = if a.is_major || a.is_branch {
            a.new_ref.red().to_string()
        } else {
            a.new_ref.green().to_string()
        };
        let mut line = format!(
            "{}:{} {}: {} -> {}",
            a.file.cyan(),
            a.line,
            a.action_name().bold(),
            a.current_ref.bright_black(),
            target
        );
        if a.new_ref != a.new_version {
            line.push_str(&format!(" [{}]", a.new_version.bright_black()));
        }
        if let Some(vc) = &a.version_comment {
            line.push_str(&format!(" #{}", vc.bright_black()));
        }
        if a.sha_mismatch {
            line.push_str(&format!(" {}", "(SHA/comment mismatch)".red()));
        }
        if a.is_branch {
            line.push_str(&format!(" {}", "(unpinned branch ref)".yellow()));
        }
        printer.info(&line);
    }

    match crate::rewrite::apply(&actions, &selected) {
        Ok(applied) => {
            let files = actions
                .iter()
                .enumerate()
                .filter_map(|(i, a)| selected.contains(&i).then_some(a.file.as_str()))
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            printer.info(&format!(
                "Updated {} workflow reference{} across {} file{}.",
                applied.to_string().yellow(),
                if applied == 1 { "" } else { "s" },
                files.to_string().yellow(),
                if files == 1 { "" } else { "s" },
            ));
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            printer.error(&format!("Could not write selected updates: {err}."));
            match &err {
                crate::rewrite::RewriteError::UpdateTargetNotFound => {
                    printer.info("Re-run actioneer so it can scan the current file contents.");
                }
                _ => {
                    printer.info("Some files may already have been written. Review your working tree before retrying.");
                }
            }
            Ok(ExitCode::FAILURE)
        }
    }
}
