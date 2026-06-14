use std::{path::PathBuf, process::ExitCode};

use crate::{
    audit::{
        audit_references,
        fix::{apply_fixes, plan_fixes},
        output::{print_report, print_report_with_fixes},
    },
    cli::AuditArgs,
    config::Config,
    discovery::discover_action_refs,
    github::GitHubTags,
};

pub fn run(args: &AuditArgs, config: &Config) -> Result<ExitCode, String> {
    let inputs = audit_inputs(args);
    let references = discover_action_refs(&inputs)?;
    let report = audit_references(&references, config);

    if args.fix {
        let github_tags = github_tags(config);
        let mut fixes = plan_fixes(&report, &github_tags)?;
        if !args.dry_run {
            apply_fixes(&mut fixes)?;
        }

        let references_after_fix = discover_action_refs(&inputs)?;
        let report_after_fix = audit_references(&references_after_fix, config);
        print_report_with_fixes(&report_after_fix, args.shared.mode, &fixes)?;

        return if report_after_fix.ok() {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::FAILURE)
        };
    }

    print_report(&report, args.shared.mode)?;

    if report.ok() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn audit_inputs(args: &AuditArgs) -> Vec<PathBuf> {
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
