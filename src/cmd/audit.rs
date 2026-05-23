use std::process::ExitCode;

use owo_colors::OwoColorize;
use thiserror::Error;

use crate::cli::{AuditArgs, GlobalArgs};
use crate::engine::{self, CheckError, CheckOptions, ResolveError};
use crate::github;
use crate::logger;
use crate::model::{PinStyle, ResolveOptions};

#[derive(Debug, Error)]
pub enum Error {}

pub fn run(global: GlobalArgs, args: AuditArgs) -> Result<ExitCode, Error> {
    let logger = logger::Logger::new(global.mode);
    let inputs = default_inputs(args.inputs, args.recursive);

    if inputs.len() == 1 {
        logger.info(format!("Scanning workflows in {}", inputs[0].bold()));
    } else {
        logger.info(format!(
            "Scanning {} input paths:",
            inputs.len().to_string().yellow()
        ));
        for input in &inputs {
            logger.debug(format!("  {}", input.bright_black()));
        }
    }

    let result = match engine::check(CheckOptions {
        paths: inputs,
        recursive: args.recursive,
        no_cache: global.no_cache,
        resolve_options: ResolveOptions {
            excludes: global.excludes,
            skip_branches: args.skip_branches,
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
            logger.error(format!("Scan failed: {}", err));
            return Ok(ExitCode::FAILURE);
        }
        Err(CheckError::Resolve(ResolveError::GitHub { repository, source })) => {
            logger.error(format!("GitHub lookup failed for {}.", repository.bold()));
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
                        502..=504 => "GitHub appears temporarily unavailable.",
                        _ => {
                            "Retry later, or run with --dry-run/--mode json to inspect scanned references."
                        }
                    };
                    logger.info(hint);
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
            logger.warn("No action references found.");
            logger.info("Point actioneer at a workflow file or directory with `uses:` entries.");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if logger.is_json() {
        logger.json(
            serde_json::to_string(&serde_json::json!({ "updates": result.updates }))
                .expect("serializing updates payload"),
        );
        return Ok(
            if result
                .updates
                .iter()
                .any(|update| update.has_sha_mismatch() || update.is_branch_ref())
            {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            },
        );
    }

    logger.info(format!(
        "Scanned {} action reference{} across {} workflow file{}.",
        result.reference_count.to_string().yellow(),
        plural_suffix(result.reference_count),
        result.reference_file_count.to_string().yellow(),
        plural_suffix(result.reference_file_count)
    ));

    let branch_count = result.branch_ref_count;
    let mismatch_count = result
        .updates
        .iter()
        .filter(|update| update.has_sha_mismatch())
        .count();

    if branch_count == 0 && mismatch_count == 0 {
        logger.info("All references are securely pinned.");
        return Ok(ExitCode::SUCCESS);
    }

    if branch_count > 0 {
        logger.error(format!(
            "{} action reference{} use{} mutable branch refs and are insecure.",
            branch_count.to_string().yellow(),
            plural_suffix(branch_count),
            if branch_count == 1 { "s" } else { "" },
        ));
        for update in result
            .updates
            .iter()
            .filter(|update| update.is_branch_ref())
        {
            logger.error(format!(
                "{} at {}:{} uses {} (unpinned branch ref)",
                update.action.bold(),
                update.file().cyan(),
                update.line(),
                update.current.red()
            ));
        }
    }

    if mismatch_count > 0 {
        logger.error(format!(
            "{} pinned SHA{} do not match their stated versions.",
            mismatch_count.to_string().yellow(),
            plural_suffix(mismatch_count)
        ));
        for update in result
            .updates
            .iter()
            .filter(|update| update.has_sha_mismatch())
        {
            let mut line = format!(
                "{} at {}:{} uses {}",
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
            logger.error(format!("{}.", line));
        }
    }

    logger.info(
        "Run `actioneer update` to pin branch refs to version tags, or fix SHA/comment mismatches.",
    );
    Ok(ExitCode::FAILURE)
}

fn default_inputs(inputs: Vec<String>, recursive: bool) -> Vec<String> {
    if inputs.is_empty() {
        vec![if recursive { "." } else { ".github" }.to_string()]
    } else {
        inputs
    }
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
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
