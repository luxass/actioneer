use std::collections::BTreeSet;
use std::process::ExitCode;

use owo_colors::OwoColorize;
use thiserror::Error;

use crate::cli::{GlobalArgs, UpdateArgs};
use crate::cmd::prompt;
use crate::engine::rewrite::RewriteError;
use crate::engine::{self, ApplyResult, CheckError, CheckOptions, ResolveError};
use crate::github;
use crate::logger;
use crate::model::{PinStyle, ResolveOptions, ResolvedUpdate};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Check(#[from] CheckError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn run(global: GlobalArgs, args: UpdateArgs) -> Result<ExitCode, Error> {
    let logger = logger::Logger::new(global.mode);
    let inputs = default_inputs(args.inputs, args.recursive);

    if inputs.len() == 1 {
        logger.info(format!(
            "{} workflows in {}",
            "Scanning".cyan(),
            inputs[0].bold()
        ));
    } else {
        logger.info(format!(
            "{} {} input paths:",
            "Scanning".cyan(),
            inputs.len().to_string().yellow()
        ));
        for input in &inputs {
            logger.debug(format!("  - {}", input.bright_black()));
        }
    }

    let result = match engine::check(CheckOptions {
        paths: inputs,
        recursive: args.recursive,
        no_cache: global.no_cache,
        resolve_options: ResolveOptions {
            excludes: global.excludes,
            include_branches: args.include_branches,
            mode: args.update,
            style: if args.tag {
                PinStyle::Tag
            } else {
                PinStyle::Sha
            },
        },
    }) {
        Ok(result) => result,
        Err(CheckError::Scan(err)) => {
            logger.error(format!("{} {}", "Scan failed:".red(), err));
            return Ok(ExitCode::FAILURE);
        }
        Err(CheckError::Resolve(ResolveError::GitHub { repository, source })) => {
            logger.error(format!(
                "{} for {}.",
                "GitHub lookup failed".red(),
                repository.bold()
            ));
            match source {
                github::Error::HttpStatus(status) => {
                    logger.error(format!(
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
                        502 | 503 | 504 => "GitHub appears temporarily unavailable.",
                        _ => {
                            "Retry later, or run with --dry-run/--mode json to inspect scanned references."
                        }
                    };
                    logger.info(format!("{} {}", "Hint:".cyan(), hint));
                }
                github::Error::Request(err) => {
                    logger.error(format!("Request error: {}.", err.to_string().yellow()));
                    logger.info(
                        "Check network, DNS, proxy, and TLS settings. If you are unauthenticated, set GITHUB_TOKEN or run `gh auth login`.",
                    );
                }
            }
            return Ok(ExitCode::FAILURE);
        }
    };

    if result.reference_count == 0 {
        if logger.is_json() {
            logger.json(
                serde_json::to_string(&serde_json::json!({ "updates": [] }))
                    .expect("serializing updates payload"),
            );
        } else {
            logger.warn(format!("{}", "No action references found.".yellow()));
            logger.info(format!(
                "{} point actioneer at a workflow file or directory with `uses:` entries.",
                "Hint:".bright_black()
            ));
        }
        return Ok(ExitCode::SUCCESS);
    }

    if logger.is_json() {
        logger.json(
            serde_json::to_string(&serde_json::json!({ "updates": result.updates }))
                .expect("serializing updates payload"),
        );
        return Ok(ExitCode::SUCCESS);
    }

    logger.info(format!(
        "{} {} action reference{} across {} workflow file{}.",
        "Scanned".green(),
        result.reference_count.to_string().yellow(),
        plural_suffix(result.reference_count),
        result.reference_file_count.to_string().yellow(),
        plural_suffix(result.reference_file_count)
    ));
    logger.info(format!(
        "{} {} available update{} across {} workflow file{}.",
        "Resolved".green(),
        result.updates.len().to_string().yellow(),
        plural_suffix(result.updates.len()),
        update_file_count(&result.updates).to_string().yellow(),
        plural_suffix(update_file_count(&result.updates))
    ));

    let mismatch_count = result
        .updates
        .iter()
        .filter(|update| update.has_sha_mismatch())
        .count();
    if mismatch_count > 0 {
        logger.warn(format!(
            "{} {} pinned SHA{} do not match their version comments.",
            "Warning:".yellow(),
            mismatch_count.to_string().yellow(),
            plural_suffix(mismatch_count)
        ));
        for update in result
            .updates
            .iter()
            .filter(|update| update.has_sha_mismatch())
        {
            let mut line = format!(
                "  - {} at {}:{} uses {}",
                update.action.bold(),
                update.file().cyan(),
                update.line(),
                update.current.red()
            );
            if update.has_version_comment() {
                line.push_str(&format!(" but says {}", update.version_comment().yellow()));
            }
            if update.has_current_ref() {
                line.push_str(&format!(
                    "; expected {}",
                    short_sha(update.current_ref()).green()
                ));
            }
            logger.warn(format!("{line}."));
        }
    }

    if result.skipped_branches > 0 {
        logger.warn(format!(
            "{} {} action reference{} use{} branch refs (e.g. @main, @master) and were skipped. \
             Use --include-branches to include them.",
            "Warning:".yellow(),
            result.skipped_branches.to_string().yellow(),
            plural_suffix(result.skipped_branches),
            if result.skipped_branches == 1 { "s" } else { "" },
        ));
    }

    if global.dry_run {
        logger.info(format!(
            "{}: {} scanned reference{}, {} available update{}.",
            "Preview".cyan(),
            result.reference_count.to_string().yellow(),
            plural_suffix(result.reference_count),
            result.updates.len().to_string().yellow(),
            plural_suffix(result.updates.len())
        ));
        for update in &result.updates {
            let target = if update.is_major_update() {
                update.display_target().red().to_string()
            } else {
                update.display_target().green().to_string()
            };
            let mut line = format!(
                "  - {} [{}]: {} -> {} ({}:{})",
                update.action.bold(),
                update.job.bright_black(),
                update.current.yellow(),
                target,
                update.file().bright_black(),
                update.line()
            );
            if update.has_version_comment() {
                line.push_str(&format!(" #{}", update.version_comment().bright_black()));
            }
            if update.has_sha_mismatch() {
                line.push_str(&format!(" {}", "(SHA/comment mismatch)".red()));
            }
            logger.info(line);
        }
        return Ok(ExitCode::SUCCESS);
    }

    if result.updates.is_empty() {
        logger.info(format!("{}", "Everything is already up to date.".green()));
        return Ok(ExitCode::SUCCESS);
    }

    let selected = if args.yes {
        (0..result.updates.len()).collect()
    } else {
        match prompt::select_updates(&result.updates) {
            Ok(selected) => selected,
            Err(prompt::Error::NotATerminal) => {
                logger.error(format!(
                    "{} in this terminal.",
                    "Interactive selection is not available".yellow()
                ));
                logger.info(format!(
                    "Use {}, {}, or {}.",
                    "--yes".cyan(),
                    "--dry-run".cyan(),
                    "--mode json".cyan()
                ));
                return Ok(ExitCode::FAILURE);
            }
            Err(prompt::Error::Canceled) => {
                logger.warn(format!("{}", "Selection canceled.".yellow()));
                return Ok(ExitCode::SUCCESS);
            }
            Err(prompt::Error::Interrupted) => {
                logger.warn(format!("{}", "Selection interrupted.".yellow()));
                return Ok(ExitCode::FAILURE);
            }
            Err(prompt::Error::Io(err)) => return Err(Error::Io(err)),
        }
    };

    if selected.is_empty() {
        logger.info(format!(
            "{}",
            "No updates selected. No files were changed.".yellow()
        ));
        return Ok(ExitCode::SUCCESS);
    }

    logger.info(format!(
        "{} {} selected update{}:",
        "Applying".cyan(),
        selected.len().to_string().yellow(),
        plural_suffix(selected.len())
    ));
    for &index in &selected {
        let update = &result.updates[index];
        let target = if update.is_major_update() {
            update.display_target().red().to_string()
        } else {
            update.display_target().green().to_string()
        };
        let mut line = format!(
            "  - {}:{} [{}] {} {} -> {}",
            update.file().cyan(),
            update.line(),
            update.job.bright_black(),
            update.action.bold(),
            summarize_ref(&update.current).bright_black(),
            target
        );
        if update.has_version_comment() {
            line.push_str(&format!(" #{}", update.version_comment().bright_black()));
        }
        if update.has_sha_mismatch() {
            line.push_str(&format!(" {}", "(SHA/comment mismatch)".red()));
        }
        logger.info(line);
    }

    match engine::apply(&result.updates, &selected) {
        Ok(ApplyResult {
            applied,
            selected_files,
        }) => {
            logger.info(format!(
                "{} {} workflow reference{} across {} file{}.",
                "Updated".green(),
                applied.to_string().yellow(),
                plural_suffix(applied),
                selected_files.to_string().yellow(),
                plural_suffix(selected_files)
            ));
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            logger.error(format!(
                "{} {}.",
                "Could not write selected updates:".red(),
                err
            ));
            match err {
                RewriteError::UpdateTargetNotFound => logger.info(format!(
                    "{}",
                    "Fix: re-run actioneer so it can scan the current file contents.".cyan()
                )),
                _ => logger.info(format!(
                    "{}",
                    "Check: some files may already have been written. Review your working tree before retrying.".cyan()
                )),
            }
            Ok(ExitCode::FAILURE)
        }
    }
}

fn default_inputs(inputs: Vec<String>, recursive: bool) -> Vec<String> {
    if inputs.is_empty() {
        vec![if recursive { "." } else { ".github" }.to_string()]
    } else {
        inputs
    }
}

fn update_file_count(updates: &[ResolvedUpdate]) -> usize {
    updates
        .iter()
        .map(|update| update.file())
        .collect::<BTreeSet<_>>()
        .len()
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn summarize_ref(reference: &str) -> &str {
    if reference.len() >= 16 && reference.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        short_sha(reference)
    } else {
        reference
    }
}

#[cfg(test)]
mod tests {
    use super::default_inputs;

    #[test]
    fn defaults_to_dot_github() {
        assert_eq!(
            vec![String::from(".github")],
            default_inputs(Vec::new(), false)
        );
    }

    #[test]
    fn defaults_to_dot_when_recursive() {
        assert_eq!(vec![String::from(".")], default_inputs(Vec::new(), true));
    }
}
