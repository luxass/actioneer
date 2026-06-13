use std::collections::HashMap;
use std::process::ExitCode;
use std::time::SystemTime;

use owo_colors::OwoColorize;

use crate::actions::{
    ActionReference, ActionUpdate, ResolveConfig, Tag, UpdateNote, is_likely_sha, resolve,
};
use crate::cli::{GlobalArgs, MinReleaseAge, UpdateArgs};
use crate::cmd::{
    apply_filters, default_inputs, describe_sha_mismatch, discover_actions, fetch_tags_reporting,
    plural, report_github_error, resolve_config,
};
use crate::github::{Error as GitHubError, GitHubClient};
use crate::terminal::display::{Printer, print_json, short_sha, update_file_count};
use crate::terminal::prompt;
use crate::workflows::{PatchError, apply_patches};

pub fn run(global: GlobalArgs, args: UpdateArgs, gh: GitHubClient) -> ExitCode {
    let printer = Printer::new(global.mode);
    let inputs = default_inputs(args.scan.inputs.clone(), args.scan.recursive);

    let actions = match discover_actions(&printer, global.mode, &inputs, args.scan.recursive) {
        Ok(actions) => actions,
        Err(code) => return code,
    };

    let mut tags = match fetch_tags_reporting(&printer, &actions, &gh) {
        Ok(tags) => tags,
        Err(code) => return code,
    };

    let config = resolve_config(&global, &args.scan);
    let updates = match resolve_update_candidates(
        &actions,
        &mut tags,
        &config,
        &args.scan.filters,
        args.min_release_age,
        &gh,
        SystemTime::now(),
    ) {
        Ok(updates) => updates,
        Err(err) => {
            printer.error(&format!(
                "GitHub release date lookup failed for {}/{}@{}.",
                err.owner.bold(),
                err.name.bold(),
                err.tag.bold()
            ));
            report_github_error(&printer, &err.error);
            return ExitCode::FAILURE;
        }
    };

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

    let selected = if args.scan.yes {
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

#[derive(Debug)]
struct FetchReleaseDateError {
    owner: String,
    name: String,
    tag: String,
    error: GitHubError,
}

fn resolve_update_candidates(
    actions: &[ActionReference],
    tags: &mut HashMap<(String, String), Vec<Tag>>,
    config: &ResolveConfig,
    filters: &[String],
    min_release_age: Option<MinReleaseAge>,
    gh: &GitHubClient,
    now: SystemTime,
) -> Result<Vec<ActionUpdate>, FetchReleaseDateError> {
    let Some(min_release_age) = min_release_age else {
        return Ok(resolve_filtered(actions, tags, config, filters));
    };

    let mut release_times: HashMap<(String, String, String), SystemTime> = HashMap::new();
    loop {
        let updates = resolve_filtered(actions, tags, config, filters);
        let mut too_new = Vec::new();

        for update in &updates {
            let key = (
                update.action.owner.clone(),
                update.action.name.clone(),
                update.new_version.clone(),
            );
            let release_time = if let Some(release_time) = release_times.get(&key) {
                *release_time
            } else {
                let release_time = gh
                    .fetch_tag_release_time(
                        &update.action.owner,
                        &update.action.name,
                        &update.new_version,
                    )
                    .map_err(|error| FetchReleaseDateError {
                        owner: update.action.owner.clone(),
                        name: update.action.name.clone(),
                        tag: update.new_version.clone(),
                        error,
                    })?;
                release_times.insert(key.clone(), release_time);
                release_time
            };

            if !release_is_old_enough(release_time, min_release_age, now) {
                too_new.push(key);
            }
        }

        if too_new.is_empty() {
            return Ok(updates);
        }

        for (owner, name, tag) in too_new {
            if let Some(repo_tags) = tags.get_mut(&(owner, name)) {
                repo_tags.retain(|candidate| candidate.name != tag);
            }
        }
    }
}

fn resolve_filtered(
    actions: &[ActionReference],
    tags: &HashMap<(String, String), Vec<Tag>>,
    config: &ResolveConfig,
    filters: &[String],
) -> Vec<ActionUpdate> {
    apply_filters(resolve(actions, tags, config), filters)
}

fn release_is_old_enough(
    release_time: SystemTime,
    min_release_age: MinReleaseAge,
    now: SystemTime,
) -> bool {
    now.duration_since(release_time)
        .is_ok_and(|age| age >= min_release_age.as_duration())
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
    use std::time::{Duration, UNIX_EPOCH};

    use crate::actions::{ActionReference, PinStyle, UpdateMode, fixtures};
    use crate::terminal::display::strip_ansi;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[test]
    fn release_age_requires_release_to_be_old_enough() {
        let now = UNIX_EPOCH + Duration::from_secs(10 * 60);
        let old_release = UNIX_EPOCH;
        let new_release = UNIX_EPOCH + Duration::from_secs(9 * 60);
        let min_age = MinReleaseAge::from_duration(Duration::from_secs(5 * 60));

        assert!(release_is_old_enough(old_release, min_age, now));
        assert!(!release_is_old_enough(new_release, min_age, now));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn min_release_age_falls_back_to_newest_old_enough_tag() {
        let server = MockServer::start().await;
        mock_lightweight_release_time(&server, "v3.0.0", "sha30", "2024-01-09T00:00:00Z").await;
        mock_lightweight_release_time(&server, "v2.0.0", "sha20", "2024-01-01T00:00:00Z").await;

        let action = ActionReference {
            current_ref: "v1.0.0".into(),
            ..fixtures::reference()
        };
        let actions = vec![action];
        let mut tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![
                fixtures::tag("v3.0.0", "sha30", 3, 0, 0),
                fixtures::tag("v2.0.0", "sha20", 2, 0, 0),
                fixtures::tag("v1.0.0", "sha10", 1, 0, 0),
            ],
        )]);
        let config = ResolveConfig {
            excludes: vec![],
            skip_branches: false,
            mode: UpdateMode::Major,
            style: PinStyle::Sha,
        };
        let now = UNIX_EPOCH + Duration::from_secs(1_704_844_800);
        let min_age = MinReleaseAge::from_duration(Duration::from_secs(7 * 24 * 60 * 60));
        let base_url = server.uri();
        let updates = tokio::task::block_in_place(|| {
            let gh = GitHubClient::new_for_test(false, base_url, None);
            resolve_update_candidates(&actions, &mut tags, &config, &[], Some(min_age), &gh, now)
                .unwrap()
        });

        assert_eq!(1, updates.len());
        assert_eq!("v2.0.0", updates[0].new_version);
        assert_eq!("sha20", updates[0].new_ref);
    }

    async fn mock_lightweight_release_time(server: &MockServer, tag: &str, sha: &str, date: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/repos/actions/checkout/git/ref/tags/{tag}")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(r#"{{"object":{{"type":"commit","sha":"{sha}"}}}}"#),
                "application/json",
            ))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/repos/actions/checkout/git/commits/{sha}")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(r#"{{"committer":{{"date":"{date}"}}}}"#),
                "application/json",
            ))
            .mount(server)
            .await;
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
