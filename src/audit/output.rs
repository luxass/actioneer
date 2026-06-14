use serde::Serialize;

use crate::{
    audit::{AuditAction, AuditFinding, AuditReport},
    cli::Mode,
};

pub fn print_report(report: &AuditReport, mode: Option<Mode>) -> Result<(), String> {
    if mode == Some(Mode::Json) {
        print_json_report(report)
    } else {
        print_human_report(report);
        Ok(())
    }
}

fn print_human_report(report: &AuditReport) {
    if report.ok() {
        println!("No audit findings.");
        return;
    }

    for finding in &report.findings {
        println!(
            "{}:{}: {}: {}@{}",
            finding.file,
            finding.line,
            finding.kind.as_str(),
            finding.action.repo,
            finding.action.ref_name
        );
    }
}

fn print_json_report(report: &AuditReport) -> Result<(), String> {
    let json = AuditReportJson {
        schema_version: 1,
        command: "audit",
        ok: report.ok(),
        summary: AuditSummaryJson {
            references: report.references,
            findings: report.findings.len(),
            fixable: report.fixable_count(),
        },
        findings: report
            .findings
            .iter()
            .map(AuditFindingJson::from_finding)
            .collect(),
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json)
            .map_err(|error| format!("failed to render audit JSON: {error}"))?
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct AuditReportJson {
    schema_version: u8,
    command: &'static str,
    ok: bool,
    summary: AuditSummaryJson,
    findings: Vec<AuditFindingJson>,
}

#[derive(Debug, Serialize)]
struct AuditSummaryJson {
    references: usize,
    findings: usize,
    fixable: usize,
}

#[derive(Debug, Serialize)]
struct AuditFindingJson {
    id: String,
    kind: &'static str,
    severity: &'static str,
    file: String,
    line: usize,
    action: AuditActionJson,
    message: String,
    recommendation: String,
    fixable: bool,
    expected_sha: Option<String>,
}

impl AuditFindingJson {
    fn from_finding(finding: &AuditFinding) -> Self {
        Self {
            id: finding.id.clone(),
            kind: finding.kind.as_str(),
            severity: finding.severity.as_str(),
            file: finding.file.clone(),
            line: finding.line,
            action: AuditActionJson::from_action(&finding.action),
            message: finding.message.clone(),
            recommendation: finding.recommendation.clone(),
            fixable: finding.fixable,
            expected_sha: finding.expected_sha.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AuditActionJson {
    owner: String,
    name: String,
    repo: String,
    path: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

impl AuditActionJson {
    fn from_action(action: &AuditAction) -> Self {
        Self {
            owner: action.owner.clone(),
            name: action.name.clone(),
            repo: action.repo.clone(),
            path: action.path.clone(),
            ref_name: action.ref_name.clone(),
        }
    }
}
