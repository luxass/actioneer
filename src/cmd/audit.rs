use std::process::ExitCode;

use owo_colors::OwoColorize;

use crate::actions::resolve;
use crate::cli::{GlobalArgs, ScanArgs};
use crate::cmd::{
    default_inputs, describe_sha_mismatch, discover_actions, fetch_tags_reporting, resolve_config,
};
use crate::github::GitHubClient;
use crate::terminal::display::{Printer, print_json};

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

    let findings: Vec<_> = resolve(&actions, &tags, &resolve_config(&global, &args))
        .into_iter()
        .filter(|a| a.is_branch || a.sha_mismatch)
        .collect();

    if global.mode.is_json() {
        print_json(&findings);
        return if findings.iter().any(|a| a.is_security_sensitive()) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    let branch_count = findings.iter().filter(|a| a.is_branch).count();
    let mismatch_count = findings.iter().filter(|a| a.sha_mismatch).count();

    if branch_count == 0 && mismatch_count == 0 {
        printer.info("All references are securely pinned.");
        return ExitCode::SUCCESS;
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
            printer.error(&describe_sha_mismatch(a));
        }
    }

    printer.info(
        "Run `actioneer update` to pin branch refs to version tags, or fix SHA/comment mismatches.",
    );
    ExitCode::FAILURE
}
