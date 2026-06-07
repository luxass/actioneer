use std::collections::{HashMap, HashSet};
use std::process::ExitCode;

use owo_colors::OwoColorize;

use crate::actions::{ResolveConfig, Tag, resolve};
use crate::cli::{GlobalArgs, ScanArgs};
use crate::cmd::default_inputs;
use crate::github::{Error as GitHubError, GitHubClient};
use crate::terminal::display::{Printer, print_json, short_sha};
use crate::workflows::find_action_references;

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

    let actions = match find_action_references(&inputs, args.recursive) {
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
    let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
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
                            401 => "Set GITHUB_TOKEN or run `gh auth login`.",
                            403 => {
                                "Rate limit or access restriction. Set GITHUB_TOKEN or run `gh auth login`."
                            }
                            404 => "Repository not found or not publicly accessible.",
                            429 => "GitHub is rate limiting these requests.",
                            502..=504 => "GitHub appears temporarily unavailable.",
                            _ => "Retry later.",
                        };
                        printer.info(hint);
                    }
                    GitHubError::Request(err) => {
                        printer.error(&format!("Request error: {}.", err.to_string().yellow()));
                        printer.info("Check network, DNS, proxy, and TLS settings.");
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
    let findings: Vec<_> = resolve(&actions, &tags, &resolve_config)
        .into_iter()
        .filter(|a| a.is_branch || a.sha_mismatch)
        .collect();

    if global.mode.is_json() {
        print_json(&findings);
        return Ok(if findings.iter().any(|a| a.sha_mismatch || a.is_branch) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }

    let branch_count = findings.iter().filter(|a| a.is_branch).count();
    let mismatch_count = findings.iter().filter(|a| a.sha_mismatch).count();

    if branch_count == 0 && mismatch_count == 0 {
        printer.info("All references are securely pinned.");
        return Ok(ExitCode::SUCCESS);
    }

    if branch_count > 0 {
        printer.error(&format!(
            "{} action reference{} {} mutable branch refs and {} insecure.",
            branch_count.to_string().yellow(),
            if branch_count == 1 { "" } else { "s" },
            if branch_count == 1 { "uses" } else { "use" },
            if branch_count == 1 { "is" } else { "are" },
        ));
        for a in findings.iter().filter(|a| a.is_branch) {
            printer.error(&format!(
                "{} at {}:{} uses {} (unpinned branch ref)",
                a.action_name().bold(),
                a.action.file.cyan(),
                a.action.line,
                a.action.current_ref.red()
            ));
        }
    }

    if mismatch_count > 0 {
        printer.error(&format!(
            "{} pinned SHA{} do not match their stated versions.",
            mismatch_count.to_string().yellow(),
            if mismatch_count == 1 { "" } else { "s" },
        ));
        for a in findings.iter().filter(|a| a.sha_mismatch) {
            let mut line = format!(
                "{} at {}:{} uses {}",
                a.action_name().bold(),
                a.action.file.cyan(),
                a.action.line,
                a.action.current_ref.red()
            );
            if let Some(vc) = &a.action.version_comment {
                line.push_str(&format!(" but says {}", vc.yellow()));
            }
            if !a.expected_sha.is_empty() {
                line.push_str(&format!(
                    "; expected {}",
                    short_sha(&a.expected_sha).green()
                ));
            }
            printer.error(&format!("{line}."));
        }
    }

    printer.info(
        "Run `actioneer update` to pin branch refs to version tags, or fix SHA/comment mismatches.",
    );
    Ok(ExitCode::FAILURE)
}
