//! Human-readable labels for planned updates.

use crate::config::PinMode;
use crate::engine::PinKind;

use super::types::{PlannedChange, ResolvedReference};

/// Label for the current pin shown in tables and the TUI.
pub fn plan_from_label(resolved: &ResolvedReference, planned: &PlannedChange) -> String {
    let reference = &resolved.located.reference;

    match reference.pin_kind {
        PinKind::FullSha | PinKind::ShortSha => {
            let sha = short_pin(&planned.from_ref);
            match planned.from_version.as_deref() {
                Some(version) => format!("{version} ({sha})"),
                None => sha,
            }
        }
        PinKind::Tag => planned
            .from_version
            .clone()
            .unwrap_or_else(|| planned.from_ref.clone()),
        PinKind::Branch => planned.from_ref.clone(),
        PinKind::Unpinned => planned.from_ref.clone(),
    }
}

/// Label for the target pin shown in tables and the TUI.
pub fn plan_to_label(planned: &PlannedChange, pin_mode: PinMode) -> String {
    match pin_mode {
        PinMode::Tag => planned
            .to_comment
            .clone()
            .unwrap_or_else(|| planned.to_ref.clone()),
        PinMode::Sha => match planned.to_comment.as_deref() {
            Some(ver) => format!("{ver} ({})", short_pin(&planned.to_ref)),
            None => short_pin(&planned.to_ref),
        },
    }
}

/// Truncate a display label for narrow columns, keeping the version when present.
pub fn truncate_label(label: &str, max: usize) -> String {
    if label.len() <= max {
        return label.to_string();
    }
    if let Some(paren) = label.find(" (") {
        let version = &label[..paren];
        if version.len() + 4 <= max {
            return format!("{version} (…)");
        }
    }
    format!("{}…", &label[..max.saturating_sub(1)])
}

fn short_pin(pin: &str) -> String {
    if pin.len() > 12 {
        format!("{}…", &pin[..11])
    } else {
        pin.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::PinMode;
    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{RefKind, ResolvedRef};
    use crate::scan::types::{LocatedReference, PlanReason, PlannedChange, ResolvedReference};

    use super::*;

    fn resolved(pin_kind: PinKind, git_ref: &str, line_comment: Option<&str>) -> ResolvedReference {
        ResolvedReference {
            located: LocatedReference {
                workflow_path: PathBuf::from(".github/workflows/ci.yml"),
                reference: ActionReference {
                    raw: format!("actions/checkout@{git_ref}"),
                    kind: ReferenceKind::Action,
                    pin_kind,
                    owner: Some("actions".into()),
                    repo: Some("checkout".into()),
                    subpath: None,
                    git_ref: Some(git_ref.into()),
                    step_name: None,
                    job_id: "build".into(),
                    job_name: None,
                    step_index: Some(0),
                    line: Some(10),
                    line_comment: line_comment.map(str::to_string),
                },
            },
            current: ResolvedRef {
                sha: "a".repeat(40),
                ref_kind: RefKind::Tag,
                published_at: None,
            },
            comment_match: CommentMatch::NoComment,
        }
    }

    fn planned_sha() -> PlannedChange {
        PlannedChange {
            from_ref: "df4cb1c069e1874edd31b4311f1884172cec0e10".into(),
            to_ref: "9c091bb21b7c8f2f3e4d5a6b7c8d9e0f1a2b3c4d".into(),
            from_version: Some("v6.0.3".into()),
            to_sha: "9c091bb21b7c8f2f3e4d5a6b7c8d9e0f1a2b3c4d".into(),
            to_comment: Some("v6.0.4".into()),
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        }
    }

    #[test]
    fn sha_pin_shows_version_and_short_sha() {
        let r = resolved(
            PinKind::FullSha,
            "df4cb1c069e1874edd31b4311f1884172cec0e10",
            Some("v6.0.3"),
        );
        let p = planned_sha();
        assert_eq!(plan_from_label(&r, &p), "v6.0.3 (df4cb1c069e…)");
        assert_eq!(plan_to_label(&p, PinMode::Sha), "v6.0.4 (9c091bb21b7…)");
    }

    #[test]
    fn tag_pin_shows_tags_only() {
        let r = resolved(PinKind::Tag, "v4.1.0", None);
        let p = PlannedChange {
            from_ref: "v4.1.0".into(),
            to_ref: "v4.2.0".into(),
            from_version: Some("v4.1.0".into()),
            to_sha: "b".repeat(40),
            to_comment: None,
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        };
        assert_eq!(plan_from_label(&r, &p), "v4.1.0");
        assert_eq!(plan_to_label(&p, PinMode::Tag), "v4.2.0");
    }

    #[test]
    fn branch_shows_branch_name_only() {
        let r = resolved(PinKind::Branch, "main", None);
        let p = PlannedChange {
            from_ref: "main".into(),
            to_ref: "deadbeef".repeat(5),
            from_version: None,
            to_sha: "c".repeat(40),
            to_comment: Some("v1.0.0".into()),
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        };
        assert_eq!(plan_from_label(&r, &p), "main");
    }
}
