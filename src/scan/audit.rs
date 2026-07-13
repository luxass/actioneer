//! Audit rules applied to resolved references.

use semver::Version;

use crate::config::{ActioneerConfig, PinMode, RelativeDuration, UpdateLevel};
use crate::engine::{AuditTier, CommentMatch, PinKind, ReferenceKind};
use crate::github::{Release, ResolvedRef};

use super::CommentTagResolution;
use super::pin::{
    TagShape, VersionBaseline, classify_tag, latest_on_major, parse_semver_tag, version_baseline,
};
use super::types::{AuditIssue, ResolvedReference};

/// Evaluate audit rules for one reference.
pub(super) fn evaluate(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    comment_tag_resolution: &CommentTagResolution,
) -> Vec<AuditIssue> {
    let reference = &resolved.located.reference;
    let mut issues = Vec::new();

    if reference.is_local_reusable_workflow() {
        return issues;
    }

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
        && matches!(
            reference.kind,
            ReferenceKind::Action | ReferenceKind::ReusableWorkflow
        )
    {
        issues.push(AuditIssue::NotShaPinned);
    }

    if reference.pin_kind == PinKind::Tag
        && let Some(git_ref) = reference.git_ref.as_deref()
        && matches!(classify_tag(git_ref), TagShape::MajorOnly(_))
    {
        issues.push(AuditIssue::FloatingMajorPin {
            pin: git_ref.to_string(),
        });
    }

    check_sha_provenance(resolved, comment_tag_resolution, &mut issues);
    check_comment_semantics(resolved, &mut issues);

    if let Some(min_age) = config.min_release_age
        && let Some(too_young) = check_release_age(&resolved.current, min_age)
    {
        issues.push(too_young);
    }

    if reference.kind.is_updatable() {
        check_update_blocked_by_config(resolved, releases, config, &mut issues);
    }

    issues
}

fn check_sha_provenance(
    resolved: &ResolvedReference,
    comment_tag_resolution: &CommentTagResolution,
    issues: &mut Vec<AuditIssue>,
) {
    if !matches!(
        resolved.located.reference.pin_kind,
        PinKind::FullSha | PinKind::ShortSha
    ) {
        return;
    }

    match comment_tag_resolution {
        CommentTagResolution::Failed { tag, reason } => {
            issues.push(AuditIssue::ResolutionFailed {
                message: format!("failed to resolve SHA provenance comment tag {tag:?}: {reason}"),
            });
            return;
        }
        CommentTagResolution::Resolved { .. } | CommentTagResolution::NotApplicable => {}
    }

    let current_sha = &resolved.current.sha;
    if current_sha.is_empty() {
        return;
    }

    match comment_tag_resolution {
        CommentTagResolution::Resolved { tag, sha } if sha != current_sha => {
            issues.push(AuditIssue::ShaCommentMismatch {
                comment: tag.clone(),
                expected_sha: sha.clone(),
            });
        }
        CommentTagResolution::Resolved { .. } => {}
        CommentTagResolution::Failed { .. } => unreachable!("handled before current SHA check"),
        CommentTagResolution::NotApplicable => {
            issues.push(AuditIssue::ShaProvenanceUnverifiable {
                sha: current_sha.clone(),
            });
        }
    }
}

fn check_comment_semantics(resolved: &ResolvedReference, issues: &mut Vec<AuditIssue>) {
    let reference = &resolved.located.reference;

    if !matches!(reference.pin_kind, PinKind::FullSha | PinKind::ShortSha)
        && let CommentMatch::Mismatch { comment, expected } = &resolved.comment_match
    {
        issues.push(AuditIssue::CommentMismatch {
            comment: comment.clone(),
            expected: expected.clone(),
        });
    }
}

fn check_update_blocked_by_config(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    issues: &mut Vec<AuditIssue>,
) {
    let reference = &resolved.located.reference;
    if reference.pin_kind == PinKind::Branch && config.skip_branches {
        return;
    }

    let current_ver = match version_baseline(reference) {
        VersionBaseline::Exact(v) => v,
        _ => return,
    };

    let Some(latest) = latest_on_major(releases, current_ver.major) else {
        return;
    };
    let Some(latest_ver) = parse_semver_tag(&latest.tag_name) else {
        return;
    };

    if latest_ver <= current_ver {
        return;
    }

    let allowed = select_release_for_level(releases, &current_ver, config.update);
    if allowed
        .as_ref()
        .is_none_or(|r| parse_semver_tag(&r.tag_name).is_some_and(|v| v <= current_ver))
    {
        issues.push(AuditIssue::UpdateBlockedByConfig {
            current_version: format_version_tag(&current_ver),
            available_version: latest.tag_name.clone(),
            update_level: config.update.to_string(),
        });
    }
}

fn select_release_for_level(
    releases: &[Release],
    current: &Version,
    level: UpdateLevel,
) -> Option<Release> {
    let mut candidates: Vec<&Release> = releases
        .iter()
        .filter(|r| !r.prerelease)
        .filter(|r| parse_semver_tag(&r.tag_name).is_some_and(|v| is_candidate(current, &v, level)))
        .collect();

    candidates.sort_by(|a, b| {
        let va = parse_semver_tag(&a.tag_name).unwrap();
        let vb = parse_semver_tag(&b.tag_name).unwrap();
        vb.cmp(&va)
    });

    candidates.first().map(|r| (*r).clone())
}

fn is_candidate(current: &Version, candidate: &Version, level: UpdateLevel) -> bool {
    if candidate <= current {
        return false;
    }
    match level {
        UpdateLevel::Patch => candidate.major == current.major && candidate.minor == current.minor,
        UpdateLevel::Minor => candidate.major == current.major,
        UpdateLevel::Major => true,
    }
}

fn format_version_tag(version: &Version) -> String {
    format!("v{version}")
}

fn check_release_age(current: &ResolvedRef, min_age: RelativeDuration) -> Option<AuditIssue> {
    let published_at = current.published_at.as_deref()?;
    let published =
        time::OffsetDateTime::parse(published_at, &time::format_description::well_known::Rfc3339)
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

    use crate::config::{ActioneerConfig, PinMode, UpdateLevel};
    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{RefKind, Release, ResolvedRef};
    use crate::scan::types::{LocatedReference, ResolvedReference};

    use super::*;

    fn releases() -> Vec<Release> {
        vec![
            Release {
                tag_name: "v4.1.0".into(),
                published_at: "2020-06-01T00:00:00Z".into(),
                prerelease: false,
            },
            Release {
                tag_name: "v4.2.0".into(),
                published_at: "2021-06-01T00:00:00Z".into(),
                prerelease: false,
            },
        ]
    }

    fn action_ref(
        pin_kind: PinKind,
        git_ref: &str,
        line_comment: Option<&str>,
        kind: ReferenceKind,
    ) -> ResolvedReference {
        ResolvedReference {
            located: LocatedReference {
                workflow_path: PathBuf::from(".github/workflows/ci.yml"),
                reference: ActionReference {
                    raw: format!("actions/checkout@{git_ref}"),
                    kind,
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
                published_at: Some("2020-01-01T00:00:00Z".into()),
            },
            comment_match: CommentMatch::NoComment,
        }
    }

    #[test]
    fn branch_pin_is_mutable() {
        let resolved = action_ref(PinKind::Branch, "main", None, ReferenceKind::Action);
        let issues = evaluate(
            &resolved,
            &[],
            &ActioneerConfig::default(),
            &CommentTagResolution::NotApplicable,
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, AuditIssue::MutableBranch))
        );
    }

    #[test]
    fn major_only_tag_is_floating() {
        let resolved = action_ref(PinKind::Tag, "v4", None, ReferenceKind::Action);
        let issues = evaluate(
            &resolved,
            &[],
            &ActioneerConfig::default(),
            &CommentTagResolution::NotApplicable,
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, AuditIssue::FloatingMajorPin { .. }))
        );
    }

    #[test]
    fn sha_comment_matching_release_has_no_mismatch() {
        let sha = "a".repeat(40);
        let mut resolved = action_ref(
            PinKind::FullSha,
            &sha,
            Some("v4.2.0"),
            ReferenceKind::Action,
        );
        resolved.current.sha = sha.clone();
        resolved.comment_match = CommentMatch::Match;

        let issues = evaluate(
            &resolved,
            &releases(),
            &ActioneerConfig::default(),
            &CommentTagResolution::Resolved {
                tag: "v4.2.0".into(),
                sha: sha.clone(),
            },
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn sha_major_only_comment_has_unverifiable_provenance() {
        let sha = "a".repeat(40);
        let mut resolved = action_ref(PinKind::FullSha, &sha, Some("v4"), ReferenceKind::Action);
        resolved.current.sha = sha;
        resolved.comment_match = CommentMatch::Match;

        let issues = evaluate(
            &resolved,
            &releases(),
            &ActioneerConfig::default(),
            &CommentTagResolution::NotApplicable,
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, AuditIssue::ShaProvenanceUnverifiable { .. }))
        );
    }

    #[test]
    fn sha_comment_mismatch_carries_expected_sha() {
        let sha = "a".repeat(40);
        let other = "b".repeat(40);
        let mut resolved = action_ref(
            PinKind::FullSha,
            &sha,
            Some("v4.2.0"),
            ReferenceKind::Action,
        );
        resolved.current.sha = sha;

        let issues = evaluate(
            &resolved,
            &releases(),
            &ActioneerConfig::default(),
            &CommentTagResolution::Resolved {
                tag: "v4.2.0".into(),
                sha: other.clone(),
            },
        );
        assert!(issues.iter().any(|issue| matches!(
            issue,
            AuditIssue::ShaCommentMismatch {
                comment,
                expected_sha,
            } if comment == "v4.2.0" && expected_sha == &other
        )));
    }

    #[test]
    fn update_blocked_by_config_when_patch_level_blocks_minor() {
        let resolved = action_ref(PinKind::Tag, "v4.1.0", None, ReferenceKind::Action);
        let config = ActioneerConfig {
            update: UpdateLevel::Patch,
            ..Default::default()
        };
        let issues = evaluate(
            &resolved,
            &releases(),
            &config,
            &CommentTagResolution::NotApplicable,
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, AuditIssue::UpdateBlockedByConfig { .. }))
        );
    }

    #[test]
    fn sha_mode_flags_tag_pin() {
        let resolved = action_ref(PinKind::Tag, "v4", None, ReferenceKind::Action);
        let config = ActioneerConfig {
            pin: PinMode::Sha,
            ..Default::default()
        };
        let issues = evaluate(
            &resolved,
            &[],
            &config,
            &CommentTagResolution::NotApplicable,
        );
        assert!(issues.iter().any(|i| matches!(i, AuditIssue::NotShaPinned)));
    }
}
