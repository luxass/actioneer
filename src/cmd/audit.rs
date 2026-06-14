use std::{path::PathBuf, process::ExitCode};

use serde::Serialize;

use crate::{
    cli::{AuditArgs, Mode},
    discovery::{DiscoveredActionRef, discover_action_refs},
};

pub fn run(args: &AuditArgs) -> Result<ExitCode, String> {
    let inputs = if args.inputs.is_empty() {
        vec![PathBuf::from(".github")]
    } else {
        args.inputs.clone()
    };

    let references = discover_action_refs(inputs)?;
    let findings = references
        .iter()
        .filter(|action_ref| !is_full_sha(&action_ref.ref_name))
        .enumerate()
        .map(|(index, action_ref)| AuditFindingJson::from_action_ref(index + 1, action_ref))
        .collect::<Vec<_>>();

    let ok = findings.is_empty();
    if args.shared.mode == Some(Mode::Json) {
        let report = AuditReportJson {
            schema_version: 1,
            command: "audit",
            ok,
            summary: AuditSummaryJson {
                references: references.len(),
                findings: findings.len(),
                fixable: findings.iter().filter(|finding| finding.fixable).count(),
            },
            findings,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to render audit JSON: {error}"))?
        );
    } else if ok {
        println!("No audit findings.");
    } else {
        for finding in &findings {
            println!(
                "{}:{}: {}: {}@{}",
                finding.file,
                finding.line,
                finding.kind,
                finding.action.repo,
                finding.action.ref_name
            );
        }
    }

    if ok {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn is_full_sha(ref_name: &str) -> bool {
    ref_name.len() == 40 && ref_name.chars().all(|character| character.is_ascii_hexdigit())
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
    message: &'static str,
    recommendation: &'static str,
    fixable: bool,
    expected_sha: Option<String>,
}

impl AuditFindingJson {
    fn from_action_ref(id: usize, action_ref: &DiscoveredActionRef) -> Self {
        Self {
            id: format!("finding-{id}"),
            kind: "mutable_ref",
            severity: "error",
            file: action_ref.file.display().to_string(),
            line: action_ref.line,
            action: AuditActionJson {
                owner: action_ref.owner.clone(),
                name: action_ref.name.clone(),
                repo: action_ref.repo.clone(),
                path: action_ref.path.clone(),
                ref_name: action_ref.ref_name.clone(),
            },
            message: "Action is pinned to a mutable tag",
            recommendation: "Pin to a full SHA",
            fixable: true,
            expected_sha: None,
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
