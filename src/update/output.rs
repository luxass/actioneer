use serde::Serialize;

use crate::update::{UpdateAction, UpdateCandidate, UpdatePlan, UpdateTarget};

pub fn print_json_plan(plan: &UpdatePlan) -> Result<(), String> {
    let json = UpdatePlanJson {
        schema_version: 1,
        command: "update",
        ok: true,
        summary: UpdateSummaryJson {
            references: plan.references,
            candidates: plan.candidates.len(),
            selected: plan.selected_count(),
            applied: plan.applied_count(),
        },
        candidates: plan
            .candidates
            .iter()
            .map(UpdateCandidateJson::from_candidate)
            .collect(),
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json)
            .map_err(|error| format!("failed to render update JSON: {error}"))?
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct UpdatePlanJson {
    schema_version: u8,
    command: &'static str,
    ok: bool,
    summary: UpdateSummaryJson,
    candidates: Vec<UpdateCandidateJson>,
}

#[derive(Debug, Serialize)]
struct UpdateSummaryJson {
    references: usize,
    candidates: usize,
    selected: usize,
    applied: usize,
}

#[derive(Debug, Serialize)]
struct UpdateCandidateJson {
    id: String,
    kind: &'static str,
    file: String,
    line: usize,
    action: UpdateActionJson,
    target: UpdateTargetJson,
    reason: &'static str,
    notes: Vec<&'static str>,
    selected: bool,
    applied: bool,
}

impl UpdateCandidateJson {
    fn from_candidate(candidate: &UpdateCandidate) -> Self {
        Self {
            id: candidate.id.clone(),
            kind: candidate.kind.as_str(),
            file: candidate.file.clone(),
            line: candidate.line,
            action: UpdateActionJson::from_action(&candidate.action),
            target: UpdateTargetJson::from_target(&candidate.target),
            reason: candidate.reason.as_str(),
            notes: candidate.notes.iter().map(|note| note.as_str()).collect(),
            selected: candidate.selected,
            applied: candidate.applied,
        }
    }
}

#[derive(Debug, Serialize)]
struct UpdateActionJson {
    owner: String,
    name: String,
    repo: String,
    path: String,
    current_ref: String,
}

impl UpdateActionJson {
    fn from_action(action: &UpdateAction) -> Self {
        Self {
            owner: action.owner.clone(),
            name: action.name.clone(),
            repo: action.repo.clone(),
            path: action.path.clone(),
            current_ref: action.current_ref.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct UpdateTargetJson {
    #[serde(rename = "ref")]
    ref_name: String,
    version: String,
    sha: String,
    pin: &'static str,
}

impl UpdateTargetJson {
    fn from_target(target: &UpdateTarget) -> Self {
        Self {
            ref_name: target.ref_name.clone(),
            version: target.version.clone(),
            sha: target.sha.clone(),
            pin: target.pin.as_str(),
        }
    }
}
