//! Audit rules applied to resolved references.

use crate::config::{ActioneerConfig, PinMode, RelativeDuration};
use crate::engine::{AuditTier, CommentMatch, PinKind, ReferenceKind};
use crate::github::ResolvedRef;

use super::types::{AuditIssue, ResolvedReference};

/// Evaluate audit rules for one reference.
pub fn evaluate(resolved: &ResolvedReference, config: &ActioneerConfig) -> Vec<AuditIssue> {
    let reference = &resolved.located.reference;
    let mut issues = Vec::new();

    if reference.kind.audit_tier() == AuditTier::Secondary {
        issues.push(AuditIssue::SecondaryReference {
            reference_kind: reference.kind.to_string(),
        });
        return issues;
    }

    if reference.pin_kind == PinKind::Branch {
        if config.skip_branches {
            issues.push(AuditIssue::SkippedBranch);
        } else {
            issues.push(AuditIssue::MutableBranch);
        }
    }

    if reference.pin_kind == PinKind::ShortSha {
        issues.push(AuditIssue::ShortSha);
    }

    if config.pin == PinMode::Sha
        && !matches!(reference.pin_kind, PinKind::FullSha)
        && reference.kind == ReferenceKind::Action
    {
        issues.push(AuditIssue::NotShaPinned);
    }

    match &resolved.comment_match {
        CommentMatch::Mismatch { comment, expected } => {
            issues.push(AuditIssue::CommentMismatch {
                comment: comment.clone(),
                expected: expected.clone(),
            });
        }
        CommentMatch::NoComment | CommentMatch::Match => {}
    }

    if let Some(min_age) = config.min_release_age
        && let Some(too_young) = check_release_age(&resolved.current, min_age)
    {
        issues.push(too_young);
    }

    issues
}

fn check_release_age(current: &ResolvedRef, min_age: RelativeDuration) -> Option<AuditIssue> {
    let published_at = current.published_at.as_deref()?;
    let published = time::OffsetDateTime::parse(
        published_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?;
    let now = time::OffsetDateTime::now_utc();
    let min_duration = relative_duration_to_time(min_age);
    let age = now - published;
    if age < min_duration {
        return Some(AuditIssue::ReleaseTooYoung {
            min_age: min_age.to_string(),
            published_at: published_at.to_string(),
        });
    }
    None
}

fn relative_duration_to_time(d: RelativeDuration) -> time::Duration {
    use crate::config::DurationUnit;
    match d.unit {
        DurationUnit::Days => time::Duration::days(d.amount as i64),
        DurationUnit::Hours => time::Duration::hours(d.amount as i64),
        DurationUnit::Minutes => time::Duration::minutes(d.amount as i64),
    }
}

/// Returns `true` if any issue blocks automatic updates.
pub fn blocks_update(issues: &[AuditIssue]) -> bool {
    issues.iter().any(|issue| {
        matches!(
            issue,
            AuditIssue::ReleaseTooYoung { .. }
                | AuditIssue::SkippedBranch
                | AuditIssue::ResolutionFailed { .. }
        )
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::{ActioneerConfig, PinMode};
    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{RefKind, ResolvedRef};
    use crate::scan::types::{LocatedReference, ResolvedReference};

    use super::*;

    fn action_ref(pin_kind: PinKind, git_ref: &str) -> ResolvedReference {
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
                    line_comment: None,
                },
            },
            current: ResolvedRef {
                sha: "a".repeat(40),
                ref_kind: RefKind::Tag,
                published_at: Some("2020-01-01T00:00:00Z".into()),
            },
            comment_match: CommentMatch::NoComment,
        }
    }

    #[test]
    fn branch_pin_is_mutable() {
        let resolved = action_ref(PinKind::Branch, "main");
        let issues = evaluate(&resolved, &ActioneerConfig::default());
        assert!(issues.iter().any(|i| matches!(i, AuditIssue::MutableBranch)));
    }

    #[test]
    fn skip_branches_is_informational() {
        let resolved = action_ref(PinKind::Branch, "main");
        let config = ActioneerConfig {
            skip_branches: true,
            ..Default::default()
        };
        let issues = evaluate(&resolved, &config);
        assert!(issues.iter().any(|i| matches!(i, AuditIssue::SkippedBranch)));
        assert!(!issues.iter().any(|i| matches!(i, AuditIssue::MutableBranch)));
    }

    #[test]
    fn sha_mode_flags_tag_pin() {
        let resolved = action_ref(PinKind::Tag, "v4");
        let config = ActioneerConfig {
            pin: PinMode::Sha,
            ..Default::default()
        };
        let issues = evaluate(&resolved, &config);
        assert!(issues.iter().any(|i| matches!(i, AuditIssue::NotShaPinned)));
    }

    #[test]
    fn blocks_update_on_skipped_branch() {
        assert!(blocks_update(&[AuditIssue::SkippedBranch]));
        assert!(!blocks_update(&[AuditIssue::MutableBranch]));
    }
}
