use serde_json::json;

use crate::update::{Candidate, applied_count, selected_count};

pub fn print_json_plan(references: usize, candidates: &[Candidate]) -> Result<(), String> {
    let report = json!({
        "schema_version": 1,
        "command": "update",
        "ok": true,
        "summary": {
            "references": references,
            "candidates": candidates.len(),
            "selected": selected_count(candidates),
            "applied": applied_count(candidates),
        },
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to render update JSON: {error}"))?
    );
    Ok(())
}

fn candidate_json(candidate: &Candidate) -> serde_json::Value {
    json!({
        "id": candidate.id,
        "kind": candidate.kind(),
        "file": candidate.action.file.display().to_string(),
        "line": candidate.action.line,
        "action": {
            "owner": candidate.action.owner,
            "name": candidate.action.name,
            "repo": candidate.action.repo,
            "path": candidate.action.path,
            "current_ref": candidate.action.ref_name,
        },
        "target": {
            "ref": candidate.target_ref,
            "version": candidate.version,
            "sha": candidate.sha,
            "pin": candidate.pin.as_str(),
        },
        "reason": candidate.reason(),
        "notes": candidate.notes,
        "selected": candidate.selected,
        "applied": candidate.applied,
    })
}
