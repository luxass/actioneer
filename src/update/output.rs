use serde_json::json;

use crate::update::Candidate;

pub fn print_json_plan(
    references: usize,
    candidates: &[Candidate],
    selected_indexes: &[usize],
    applied_indexes: &[usize],
) -> Result<(), String> {
    let report = json!({
        "schema_version": 1,
        "command": "update",
        "ok": true,
        "summary": {
            "references": references,
            "candidates": candidates.len(),
            "selected": selected_indexes.len(),
            "applied": applied_indexes.len(),
        },
        "candidates": candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| candidate_json(
                candidate,
                selected_indexes.contains(&index),
                applied_indexes.contains(&index),
            ))
            .collect::<Vec<_>>(),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to render update JSON: {error}"))?
    );
    Ok(())
}

fn candidate_json(candidate: &Candidate, selected: bool, applied: bool) -> serde_json::Value {
    json!({
        "id": candidate.id,
        "kind": "version_update",
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
        "reason": "newer_version_available",
        "notes": candidate.notes,
        "selected": selected,
        "applied": applied,
    })
}
