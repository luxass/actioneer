use std::collections::{HashMap, HashSet};
use std::process::ExitCode;

use owo_colors::OwoColorize;

use crate::actions::{ActionReference, ActionUpdate, ResolveConfig, Tag};
use crate::cli::{GlobalArgs, Mode, ScanArgs};
use crate::github::{Error as GitHubError, GitHubClient};
use crate::terminal::display::{Printer, print_json, short_sha};
use crate::workflows::find_action_references;

pub mod audit;
pub mod update;
pub mod version;

pub(crate) struct FetchTagsError {
    pub owner: String,
    pub name: String,
    pub error: GitHubError,
}

pub(crate) fn fetch_tags_for_actions(
    actions: &[ActionReference],
    gh: &GitHubClient,
) -> Result<HashMap<(String, String), Vec<Tag>>, FetchTagsError> {
    let repos: HashSet<(String, String)> = actions
        .iter()
        .map(|a| (a.owner.clone(), a.name.clone()))
        .collect();
    let mut tags = HashMap::new();
    for (owner, name) in repos {
        match gh.fetch_tags(&owner, &name) {
            Ok(repo_tags) => {
                tags.insert((owner, name), repo_tags);
            }
            Err(error) => {
                return Err(FetchTagsError { owner, name, error });
            }
        }
    }
    Ok(tags)
}

pub(crate) fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn default_inputs(inputs: Vec<String>, recursive: bool) -> Vec<String> {
    if inputs.is_empty() {
        vec![if recursive { "." } else { ".github" }.to_string()]
    } else {
        inputs
    }
}

pub(crate) fn resolve_config(global: &GlobalArgs, args: &ScanArgs) -> ResolveConfig {
    ResolveConfig {
        excludes: global.excludes.clone(),
        skip_branches: args.skip_branches,
        mode: args.update,
        style: args.pin,
    }
}

pub(crate) fn apply_filters(updates: Vec<ActionUpdate>, filters: &[String]) -> Vec<ActionUpdate> {
    if filters.is_empty() {
        return updates;
    }
    updates
        .into_iter()
        .filter(|u| {
            let key = format!("{}/{}", u.action.owner, u.action.name);
            filters.iter().any(|f| *f == key)
        })
        .collect()
}

pub(crate) fn discover_actions(
    printer: &Printer,
    mode: Mode,
    inputs: &[String],
    recursive: bool,
) -> Result<Vec<ActionReference>, ExitCode> {
    if inputs.len() == 1 {
        printer.info(&format!("Scanning workflows in {}", inputs[0].bold()));
    } else {
        printer.info(&format!(
            "Scanning {} input paths:",
            inputs.len().to_string().yellow()
        ));
        for input in inputs {
            printer.debug(&format!("  {}", input.bright_black()));
        }
    }

    let actions = match find_action_references(inputs, recursive) {
        Ok(a) => a,
        Err(err) => {
            printer.error(&format!("Scan failed: {err}"));
            return Err(ExitCode::FAILURE);
        }
    };

    if actions.is_empty() {
        if mode.is_json() {
            print_json(&[]);
        } else {
            printer.warn("No action references found.");
            printer.info("Point actioneer at a workflow file or directory with `uses:` entries.");
        }
        return Err(ExitCode::SUCCESS);
    }

    Ok(actions)
}

pub(crate) fn fetch_tags_reporting(
    printer: &Printer,
    actions: &[ActionReference],
    gh: &GitHubClient,
) -> Result<HashMap<(String, String), Vec<Tag>>, ExitCode> {
    fetch_tags_for_actions(actions, gh).map_err(|err| {
        printer.error(&format!(
            "GitHub lookup failed for {}/{}.",
            err.owner.bold(),
            err.name.bold()
        ));
        match &err.error {
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
            GitHubError::Request(error) => {
                printer.error(&format!("Request error: {}.", error.to_string().yellow()));
                printer.info("Check network, DNS, proxy, and TLS settings. If you are unauthenticated, set GITHUB_TOKEN or run `gh auth login`.");
            }
        }
        ExitCode::FAILURE
    })
}

pub(crate) fn describe_sha_mismatch(update: &ActionUpdate) -> String {
    let mut line = format!(
        "{} at {}:{} uses {}",
        update.action_name().bold(),
        update.action.file.cyan(),
        update.action.line,
        update.action.current_ref.red()
    );
    if let Some(vc) = &update.action.version_comment {
        line.push_str(&format!(" but says {}", vc.yellow()));
    }
    if !update.expected_sha.is_empty() {
        line.push_str(&format!(
            "; expected {}",
            short_sha(&update.expected_sha).green()
        ));
    }
    line.push('.');
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::fixtures;

    #[test]
    fn empty_recursive_defaults_to_dot() {
        assert_eq!(vec!["."], default_inputs(vec![], true));
    }

    #[test]
    fn empty_non_recursive_defaults_to_github() {
        assert_eq!(vec![".github"], default_inputs(vec![], false));
    }

    #[test]
    fn explicit_inputs_returned_verbatim() {
        assert_eq!(vec!["ci.yml"], default_inputs(vec!["ci.yml".into()], true));
    }

    fn make_update(owner: &str, name: &str) -> ActionUpdate {
        fixtures::update(ActionReference {
            owner: owner.into(),
            name: name.into(),
            ..fixtures::reference()
        })
    }

    #[test]
    fn apply_filters_empty_returns_all() {
        let updates = vec![
            make_update("actions", "checkout"),
            make_update("actions", "setup-node"),
        ];
        assert_eq!(2, apply_filters(updates, &[]).len());
    }

    #[test]
    fn apply_filters_single_match() {
        let updates = vec![
            make_update("actions", "checkout"),
            make_update("actions", "setup-node"),
        ];
        let result = apply_filters(updates, &["actions/checkout".into()]);
        assert_eq!(1, result.len());
        assert_eq!("checkout", result[0].action.name);
    }

    #[test]
    fn apply_filters_multiple_matches() {
        let updates = vec![
            make_update("actions", "checkout"),
            make_update("actions", "setup-node"),
            make_update("actions", "cache"),
        ];
        let result = apply_filters(
            updates,
            &["actions/checkout".into(), "actions/cache".into()],
        );
        assert_eq!(2, result.len());
    }

    #[test]
    fn apply_filters_no_match_returns_empty() {
        let updates = vec![make_update("actions", "checkout")];
        let result = apply_filters(updates, &["actions/setup-node".into()]);
        assert!(result.is_empty());
    }

    #[test]
    fn apply_filters_requires_exact_owner_name() {
        let updates = vec![make_update("actions", "checkout")];
        // partial matches should not work
        let result = apply_filters(updates, &["checkout".into()]);
        assert!(result.is_empty());
    }
}
