use std::{path::PathBuf, process::ExitCode};

use crate::{
    cli::{Mode, UpdateArgs},
    config::Config,
    discovery::{discover_action_refs, filter_action_refs},
    github::GitHubTags,
    patch::{apply_patch_edits, update_patch_edits},
    tui,
    update::{all_candidate_indexes, output::print_json_plan, plan_update_candidates},
};

pub fn run(args: &UpdateArgs, config: &Config) -> Result<ExitCode, String> {
    let mode = effective_mode(args, config);
    require_non_interactive_confirmation(mode, args)?;

    let references = filter_action_refs(
        discover_action_refs(&update_inputs(args, config))?,
        &config.filter,
        &config.exclude,
    );
    let github_tags = github_tags(config);
    let candidates = plan_update_candidates(&references, config, &github_tags)?;
    let selected_indexes = selected_indexes(args, config, &candidates)?;
    let mut applied_indexes = Vec::new();

    if args.yes && !args.dry_run {
        let edits = update_patch_edits(&candidates, &selected_indexes);
        apply_patch_edits(&edits)?;
        applied_indexes = selected_indexes.clone();
    }

    match mode {
        Mode::Json => print_json_plan(
            references.len(),
            &candidates,
            &selected_indexes,
            &applied_indexes,
        )?,
        _ => print_plain_plan(&candidates),
    }

    Ok(ExitCode::SUCCESS)
}

fn effective_mode(args: &UpdateArgs, config: &Config) -> Mode {
    args.shared.mode.or(config.mode).unwrap_or(Mode::Tui)
}

fn require_non_interactive_confirmation(mode: Mode, args: &UpdateArgs) -> Result<(), String> {
    if matches!(mode, Mode::Plain | Mode::Json) && !args.yes && !args.dry_run {
        return Err("update --mode plain/json requires --yes or --dry-run".to_string());
    }

    Ok(())
}

fn selected_indexes(
    args: &UpdateArgs,
    config: &Config,
    candidates: &[crate::update::Candidate],
) -> Result<Vec<usize>, String> {
    if args.yes || args.dry_run {
        return Ok(all_candidate_indexes(candidates));
    }

    if effective_mode(args, config) == Mode::Tui {
        return tui::select(candidates);
    }

    Ok(Vec::new())
}

fn update_inputs(args: &UpdateArgs, config: &Config) -> Vec<PathBuf> {
    if args.inputs.is_empty() {
        if config.recursive {
            vec![PathBuf::from(".")]
        } else {
            vec![PathBuf::from(".github")]
        }
    } else {
        args.inputs.clone()
    }
}

fn github_tags(config: &Config) -> GitHubTags {
    let cache_dir = std::env::var_os("ACTIONEER_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".actioneer/cache"));
    let mut github_tags = GitHubTags::new(cache_dir)
        .with_no_cache(config.no_cache)
        .with_offline(config.offline);

    if let Ok(api_base_url) = std::env::var("ACTIONEER_GITHUB_API_BASE_URL") {
        github_tags = github_tags.with_api_base_url(api_base_url);
    }

    github_tags
}

fn print_plain_plan(candidates: &[crate::update::Candidate]) {
    println!("{} update candidate(s)", candidates.len());
}
