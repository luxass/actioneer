//! Audit command execution and rendering.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cache::cache_dir;
use crate::config::{ActioneerConfig, OutputMode};
use crate::github::GitHubClient;
use crate::scan::{AuditIssue, ScanReport, scan_workspace};

/// Scan the selected workflows, render audit results, and return the CLI status.
///
/// This reads workflows relative to the current directory and may use the
/// GitHub cache or network. Results are written to stdout and failures to
/// stderr. The status is successful only when scanning succeeds and no audit
/// issues are found.
pub fn run(config: &ActioneerConfig, workflow_paths: &[PathBuf]) -> ExitCode {
    let root = Path::new(".");
    let client = GitHubClient::new(config, cache_dir());

    let report = match scan_workspace(root, workflow_paths, config, &client) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match config.mode {
        Some(OutputMode::Json) => render_json(&report),
        Some(OutputMode::Plain) | None => render_plain(&report),
    }

    if report.stats.issues > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn render_json(report: &ScanReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("error: failed to encode JSON: {e}"),
    }
}

fn render_plain(report: &ScanReport) {
    if report.workflows.is_empty() {
        println!("No workflow files found under .github/workflows/");
        return;
    }

    println!(
        "Scanned {} workflow(s), {} reference(s), {} issue(s)\n",
        report.stats.workflows, report.stats.references, report.stats.issues
    );

    for workflow in &report.workflows {
        let workflow_issues: usize = workflow.references.iter().map(|r| r.issues.len()).sum();
        if workflow_issues == 0 {
            continue;
        }

        println!("{}", workflow.path.display());
        if let Some(name) = &workflow.name {
            println!("  name: {name}");
        }

        for reference in &workflow.references {
            if reference.issues.is_empty() {
                continue;
            }
            let action = reference.resolved.located.reference.raw.clone();
            for issue in &reference.issues {
                println!("  - {action}: {}", issue_label(issue));
            }
        }
        println!();
    }

    if report.stats.issues == 0 {
        println!("No issues found.");
    }
}

fn issue_label(issue: &AuditIssue) -> String {
    match issue {
        AuditIssue::MutableBranch => "mutable branch pin".into(),
        AuditIssue::ShortSha => "short SHA pin".into(),
        AuditIssue::NotShaPinned => "not pinned to full SHA".into(),
        AuditIssue::CommentMismatch { comment, expected } => {
            format!("comment mismatch (got {comment:?}, expected {expected:?})")
        }
        AuditIssue::ReleaseTooYoung {
            min_age,
            published_at,
        } => {
            format!("release too young (min {min_age}, published {published_at})")
        }
        AuditIssue::SkippedBranch => "branch pin skipped by config".into(),
        AuditIssue::SecondaryReference { reference_kind } => {
            format!("secondary reference ({reference_kind})")
        }
        AuditIssue::ResolutionFailed { message } => format!("resolution failed: {message}"),
        AuditIssue::FloatingMajorPin { pin } => {
            format!("floating major-line tag ({pin})")
        }
        AuditIssue::ShaProvenanceUnverifiable { sha } => {
            format!("SHA provenance unverifiable (no full-semver comment for {sha})")
        }
        AuditIssue::ShaCommentMismatch {
            comment,
            expected_sha,
        } => {
            format!("SHA/comment mismatch (comment {comment:?} resolves to {expected_sha})")
        }
        AuditIssue::UpdateBlockedByConfig {
            current_version,
            available_version,
            update_level,
        } => format!(
            "update blocked by {update_level} level (current {current_version}, available {available_version})"
        ),
    }
}
