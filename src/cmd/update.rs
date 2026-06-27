use std::path::Path;
use std::process::ExitCode;

use crate::ansi::Colors;
use crate::cache::cache_dir;
use crate::config::{ActioneerConfig, OutputMode};
use crate::github::GitHubClient;
use crate::scan::{
    all_planned_targets, apply, plan_from_label, plan_to_label, scan_workspace, ApplyReport,
    ScanReport,
};

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

    let should_apply = config.apply || config.dry_run;

    if !should_apply {
        match config.mode {
            Some(OutputMode::Json) => render_plan_json(&report, config),
            Some(OutputMode::Plain) | None => render_plan_plain(&report, config),
        }
        return ExitCode::SUCCESS;
    }

    let targets = all_planned_targets(&report);
    let apply_report = match apply(root, &report, &targets, config, config.dry_run) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match config.mode {
        Some(OutputMode::Json) => render_apply_json(&apply_report, config.dry_run),
        Some(OutputMode::Plain) | None => print_apply_plain(&apply_report, config.dry_run),
    }

    if apply_report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn render_plan_json(report: &ScanReport, config: &ActioneerConfig) {
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

fn render_plan_plain(report: &ScanReport, config: &ActioneerConfig) {
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

fn render_apply_json(apply_report: &ApplyReport, dry_run: bool) {
    let payload = serde_json::json!({
        "dry_run": dry_run,
        "applied": apply_report.applied,
        "failures": apply_report.failures,
    });
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("error: failed to encode JSON: {e}"),
    }
}

/// Print a plain-text summary of applied updates (shared by CLI and TUI exit).
pub fn print_apply_plain(apply_report: &ApplyReport, dry_run: bool) {
    let c = Colors::stdout();

    if dry_run {
        println!("{}\n", c.warn("Dry run — no files modified."));
    }

    if apply_report.applied.is_empty() && apply_report.failures.is_empty() {
        println!("{}", c.dim("No updates to apply."));
        return;
    }

    if !apply_report.applied.is_empty() {
        let verb = if dry_run { "Would apply" } else { "Applied" };
        println!(
            "{} {} update(s):\n",
            c.bold(verb),
            apply_report.applied.len()
        );
        for change in &apply_report.applied {
            let workflow = change
                .workflow_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
            let loc = format!("{}:{}", workflow, change.line);
            println!(
                "  {}  {}  {} {} {}",
                c.workflow(&loc),
                c.action(&change.action),
                c.from(&change.from),
                c.dim("→"),
                c.to(&change.to),
            );
        }
    }

    if !apply_report.failures.is_empty() {
        println!(
            "\n{}",
            c.error(&format!("{} update(s) failed:", apply_report.failures.len()))
        );
        for failure in &apply_report.failures {
            let workflow = failure
                .workflow_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
            let loc = format!("{}:{}", workflow, failure.line);
            println!(
                "  {}  {}",
                c.workflow(&loc),
                c.error(&failure.message),
            );
        }
    }
}
