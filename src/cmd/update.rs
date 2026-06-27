use std::path::Path;
use std::process::ExitCode;

use crate::cache::cache_dir;
use crate::config::{ActioneerConfig, OutputMode};
use crate::github::GitHubClient;
use crate::scan::{plan_from_label, plan_to_label, scan_workspace, ScanReport};

pub fn run(config: &ActioneerConfig) -> ExitCode {
    let root = Path::new(".");
    let client = GitHubClient::new(config, cache_dir());

    let report = match scan_workspace(root, config, &client) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match config.mode {
        Some(OutputMode::Json) => render_json(&report, config),
        Some(OutputMode::Plain) | None => render_plain(&report, config),
    }

    ExitCode::SUCCESS
}

fn render_json(report: &ScanReport, config: &ActioneerConfig) {
    let planned: Vec<_> = report
        .planned_changes()
        .map(|(path, reference)| {
            let planned = reference.planned.as_ref().unwrap();
            serde_json::json!({
                "workflow": path,
                "action": reference.resolved.located.reference.raw,
                "from": {
                    "pin": planned.from_ref,
                    "version": planned.from_version,
                    "label": plan_from_label(&reference.resolved, planned),
                },
                "to": {
                    "pin": planned.to_ref,
                    "version": planned.to_comment,
                    "label": plan_to_label(planned, config.pin),
                },
                "reason": planned.reason,
            })
        })
        .collect();

    match serde_json::to_string_pretty(&planned) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("error: failed to encode JSON: {e}"),
    }
}

fn render_plain(report: &ScanReport, config: &ActioneerConfig) {
    let changes: Vec<_> = report.planned_changes().collect();

    if changes.is_empty() {
        println!("No updates planned.");
        println!(
            "Scanned {} workflow(s), {} reference(s).",
            report.stats.workflows, report.stats.references
        );
        if report.stats.blocked > 0 {
            println!("{} reference(s) blocked by audit rules.", report.stats.blocked);
        }
        return;
    }

    println!(
        "{} planned update(s) across {} workflow(s)\n",
        report.stats.planned, report.stats.workflows
    );
    println!(
        "{:<40} {:<35} {:<28} {}",
        "Workflow", "Action", "From", "To"
    );
    println!("{}", "-".repeat(115));

    for (path, reference) in changes {
        let planned = reference.planned.as_ref().unwrap();
        let workflow = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        let action = reference.resolved.located.reference.raw.as_str();
        let from = plan_from_label(&reference.resolved, planned);
        let to = plan_to_label(planned, config.pin);
        println!("{:<40} {:<35} {:<28} {}", workflow, action, from, to);
    }

    if report.stats.blocked > 0 {
        println!("\n{} reference(s) blocked by audit rules.", report.stats.blocked);
    }
}
