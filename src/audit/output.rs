use serde_json::json;

use crate::{
    audit::{Finding, fix::AuditFix, fixable_count},
    cli::Mode,
};

pub fn print_report(
    references: usize,
    findings: &[Finding],
    mode: Option<Mode>,
) -> Result<(), String> {
    print_report_with_fixes(references, findings, mode, &[])
}

pub fn print_report_with_fixes(
    references: usize,
    findings: &[Finding],
    mode: Option<Mode>,
    fixes: &[AuditFix],
) -> Result<(), String> {
    if mode == Some(Mode::Json) {
        print_json_report(references, findings, fixes)
    } else {
        print_human_report(findings);
        Ok(())
    }
}

fn print_human_report(findings: &[Finding]) {
    if findings.is_empty() {
        println!("No audit findings.");
        return;
    }

    for finding in findings {
        println!(
            "{}:{}: {}: {}@{}",
            finding.action.file.display(),
            finding.action.line,
            finding.kind_str(),
            finding.action.repo,
            finding.action.ref_name
        );
    }
}

fn print_json_report(
    references: usize,
    findings: &[Finding],
    fixes: &[AuditFix],
) -> Result<(), String> {
    let mut report = json!({
        "schema_version": 1,
        "command": "audit",
        "ok": findings.is_empty(),
        "summary": {
            "references": references,
            "findings": findings.len(),
            "fixable": fixable_count(findings),
        },
        "findings": findings.iter().map(finding_json).collect::<Vec<_>>(),
    });

    if !fixes.is_empty() {
        report["fixes"] = json!(fixes.iter().map(fix_json).collect::<Vec<_>>());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to render audit JSON: {error}"))?
    );
    Ok(())
}

fn finding_json(finding: &Finding) -> serde_json::Value {
    json!({
        "id": finding.id,
        "kind": finding.kind_str(),
        "severity": finding.severity(),
        "file": finding.action.file.display().to_string(),
        "line": finding.action.line,
        "action": {
            "owner": finding.action.owner,
            "name": finding.action.name,
            "repo": finding.action.repo,
            "path": finding.action.path,
            "ref": finding.action.ref_name,
        },
        "message": finding.message,
        "recommendation": finding.recommendation,
        "fixable": finding.fixable,
        "expected_sha": finding.expected_sha,
    })
}

fn fix_json(fix: &AuditFix) -> serde_json::Value {
    json!({
        "finding_id": fix.finding_id,
        "file": fix.file,
        "line": fix.line,
        "applied": fix.applied,
        "new_ref": fix.new_ref,
        "new_version_comment": fix.new_version_comment,
    })
}
