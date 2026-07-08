//! Pin classification and target-line construction for audit and plan.

use semver::Version;

use crate::config::{ActioneerConfig, PinMode};
use crate::engine::{ActionReference, PinKind};
use crate::github::Release;

use super::types::PlannedChange;

/// Shape of a version tag string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagShape {
    MajorOnly(u64),
    FullSemver(Version),
    Partial,
    NotVersion,
}

/// Baseline for semver comparisons — derived from the written pin only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionBaseline {
    Exact(Version),
    MajorOnly(u64),
    Unknown,
}

pub fn classify_tag(git_ref: &str) -> TagShape {
    let without_v = git_ref.strip_prefix('v').unwrap_or(git_ref);

    if !without_v.is_empty()
        && without_v.chars().all(|c| c.is_ascii_digit())
        && let Ok(major) = without_v.parse::<u64>()
    {
        return TagShape::MajorOnly(major);
    }

    if without_v.chars().any(|c| c == '.') {
        if let Ok(v) = Version::parse(without_v) {
            if v.pre.is_empty() {
                return TagShape::FullSemver(v);
            }
            return TagShape::Partial;
        }
        return TagShape::Partial;
    }

    TagShape::NotVersion
}

pub fn parse_semver_tag(tag: &str) -> Option<Version> {
    match classify_tag(tag) {
        TagShape::FullSemver(v) => Some(v),
        _ => None,
    }
}

/// Version label as written on the `uses:` line (tag or SHA comment).
pub fn written_version_tag(reference: &ActionReference) -> Option<String> {
    match reference.pin_kind {
        PinKind::Tag => reference.git_ref.clone(),
        PinKind::FullSha | PinKind::ShortSha => reference.line_comment.clone(),
        _ => None,
    }
}

/// Semver baseline from the written pin — no GitHub calls.
pub fn version_baseline(reference: &ActionReference) -> VersionBaseline {
    let Some(written) = written_version_tag(reference) else {
        return VersionBaseline::Unknown;
    };
    match classify_tag(&written) {
        TagShape::FullSemver(v) => VersionBaseline::Exact(v),
        TagShape::MajorOnly(major) => VersionBaseline::MajorOnly(major),
        _ => VersionBaseline::Unknown,
    }
}

pub fn build_target_value(
    reference: &ActionReference,
    planned: &PlannedChange,
    pin_mode: PinMode,
) -> String {
    let base = reference
        .raw
        .rsplit_once('@')
        .map(|(prefix, _)| prefix)
        .unwrap_or(reference.raw.as_str());
    let pin = &planned.to_ref;

    match pin_mode {
        PinMode::Sha => match planned.to_comment.as_deref() {
            Some(comment) => format!("{base}@{pin} # {comment}"),
            None => format!("{base}@{pin}"),
        },
        PinMode::Tag => {
            if reference.line_comment.is_some() {
                format!("{base}@{pin} # {pin}")
            } else {
                format!("{base}@{pin}")
            }
        }
    }
}

pub fn would_change(
    reference: &ActionReference,
    planned: &PlannedChange,
    config: &ActioneerConfig,
) -> bool {
    build_target_value(reference, planned, config.pin) != reference.raw
}

/// Pick the newest semver release on a major line (tag names only).
pub fn latest_on_major(releases: &[Release], major: u64) -> Option<&Release> {
    releases
        .iter()
        .filter(|r| !r.prerelease)
        .filter_map(|r| parse_semver_tag(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| v.major == major)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_major_only_and_full_semver() {
        assert_eq!(classify_tag("v4"), TagShape::MajorOnly(4));
        assert_eq!(
            classify_tag("v4.2.0"),
            TagShape::FullSemver(Version::new(4, 2, 0))
        );
    }

    #[test]
    fn baseline_from_written_pin() {
        let reference = ActionReference {
            raw: "actions/checkout@v4.1.0".into(),
            kind: crate::engine::ReferenceKind::Action,
            pin_kind: PinKind::Tag,
            owner: Some("actions".into()),
            repo: Some("checkout".into()),
            subpath: None,
            git_ref: Some("v4.1.0".into()),
            step_name: None,
            job_id: "build".into(),
            job_name: None,
            step_index: Some(0),
            line: Some(10),
            line_comment: None,
        };
        assert_eq!(
            version_baseline(&reference),
            VersionBaseline::Exact(Version::new(4, 1, 0))
        );
    }

    #[test]
    fn would_change_normalizes_major_only_tag() {
        let reference = ActionReference {
            raw: "actions/checkout@v4".into(),
            kind: crate::engine::ReferenceKind::Action,
            pin_kind: PinKind::Tag,
            owner: Some("actions".into()),
            repo: Some("checkout".into()),
            subpath: None,
            git_ref: Some("v4".into()),
            step_name: None,
            job_id: "build".into(),
            job_name: None,
            step_index: Some(0),
            line: Some(10),
            line_comment: None,
        };
        let planned = PlannedChange {
            from_ref: "v4".into(),
            to_ref: "v4.2.0".into(),
            from_version: Some("v4".into()),
            to_sha: "b".repeat(40),
            to_comment: None,
            reason: super::super::types::PlanReason::SemverBump {
                level: "minor".into(),
            },
        };
        let config = ActioneerConfig {
            pin: PinMode::Tag,
            ..Default::default()
        };
        assert!(would_change(&reference, &planned, &config));
    }
}
