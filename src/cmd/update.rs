use std::{path::PathBuf, process::ExitCode};

use crate::{
    cli::{Mode, UpdateArgs},
    config::Config,
    discovery::discover_action_refs,
    github::GitHubTags,
    patch::apply_update_plan,
    update::{output::print_json_plan, plan_update_candidates},
};

pub fn run(args: &UpdateArgs, config: &Config) -> Result<ExitCode, String> {
    let references = discover_action_refs(update_inputs(args))?;
    let github_tags = github_tags(config);
    let mut plan = plan_update_candidates(&references, config, &github_tags, args.dry_run || args.yes)?;

    if args.yes && !args.dry_run {
        apply_update_plan(&mut plan)?;
    }

    match args.shared.mode {
        Some(Mode::Json) => print_json_plan(&plan)?,
        _ => print_plain_plan(&plan),
    }

    Ok(ExitCode::SUCCESS)
}

fn update_inputs(args: &UpdateArgs) -> Vec<PathBuf> {
    if args.inputs.is_empty() {
        vec![PathBuf::from(".github")]
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

fn print_plain_plan(plan: &crate::update::UpdatePlan) {
    println!("{} update candidate(s)", plan.candidates.len());
}
