//! Update planning from GitHub releases and config.

use semver::Version;
use time::format_description::well_known::Rfc3339;

use crate::config::{ActioneerConfig, DurationUnit, PinMode, RelativeDuration, UpdateLevel};
use crate::engine::PinKind;
use crate::github::{GitHubClient, GitHubError, Release};

use super::audit::blocks_update;
use super::types::{AuditIssue, PlannedChange, PlanReason, ResolvedReference};

/// Propose an update for one reference, if applicable.
pub fn propose(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    client: &GitHubClient,
    issues: &[AuditIssue],
) -> Result<Option<PlannedChange>, GitHubError> {
    let reference = &resolved.located.reference;

    if !reference.kind.is_updatable() || blocks_update(issues) {
        return Ok(None);
    }

    if reference.pin_kind == PinKind::Branch && config.skip_branches {
        return Ok(None);
    }

    let Some(owner) = reference.owner.as_deref() else {
        return Ok(None);
    };
    let Some(repo) = reference.repo.as_deref() else {
        return Ok(None);
    };

    let current_tag = current_version_tag(reference);
    let Some(current_ver) = current_tag.as_deref().and_then(parse_semver_tag) else {
        return Ok(None);
    };

    let candidate = select_release(releases, &current_ver, config.update, config.min_release_age)?;
    let Some(release) = candidate else {
        return Ok(None);
    };

    if release.tag_name == current_tag.as_deref().unwrap_or("") {
        return Ok(None);
    }

    let target_resolved = client.resolve_ref(owner, repo, &release.tag_name)?;

    let (to_ref, to_comment) = match config.pin {
        PinMode::Sha => (
            target_resolved.sha.clone(),
            Some(release.tag_name.clone()),
        ),
        PinMode::Tag => (release.tag_name.clone(), None),
    };

    let from_ref = reference
        .git_ref
        .clone()
        .unwrap_or_else(|| reference.raw.clone());

    Ok(Some(PlannedChange {
        from_ref,
        to_ref,
        from_version: current_tag,
        to_sha: target_resolved.sha,
        to_comment,
        reason: PlanReason::SemverBump {
            level: config.update.to_string(),
        },
    }))
}

fn current_version_tag(reference: &crate::engine::ActionReference) -> Option<String> {
    if reference.pin_kind == PinKind::Tag {
        return reference.git_ref.clone();
    }
    if reference.pin_kind == PinKind::FullSha || reference.pin_kind == PinKind::ShortSha {
        return reference.line_comment.clone();
    }
    None
}

fn parse_semver_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(trimmed).ok()
}

fn select_release(
    releases: &[Release],
    current: &Version,
    level: UpdateLevel,
    min_age: Option<RelativeDuration>,
) -> Result<Option<Release>, GitHubError> {
    let now = time::OffsetDateTime::now_utc();

    let mut candidates: Vec<&Release> = releases
        .iter()
        .filter(|r| !r.prerelease)
        .filter(|r| {
            parse_semver_tag(&r.tag_name)
                .is_some_and(|v| is_candidate(current, &v, level))
        })
        .filter(|r| release_meets_min_age(r, min_age, now))
        .collect();

    candidates.sort_by(|a, b| {
        let va = parse_semver_tag(&a.tag_name).unwrap();
        let vb = parse_semver_tag(&b.tag_name).unwrap();
        vb.cmp(&va)
    });

    Ok(candidates.first().map(|r| (*r).clone()))
}

fn is_candidate(current: &Version, candidate: &Version, level: UpdateLevel) -> bool {
    if candidate <= current {
        return false;
    }
    match level {
        UpdateLevel::Patch => {
            candidate.major == current.major && candidate.minor == current.minor
        }
        UpdateLevel::Minor => candidate.major == current.major,
        UpdateLevel::Major => true,
    }
}

fn release_meets_min_age(
    release: &Release,
    min_age: Option<RelativeDuration>,
    now: time::OffsetDateTime,
) -> bool {
    let Some(min_age) = min_age else {
        return true;
    };
    let Ok(published) = time::OffsetDateTime::parse(&release.published_at, &Rfc3339) else {
        return false;
    };
    let min_duration = match min_age.unit {
        DurationUnit::Days => time::Duration::days(min_age.amount as i64),
        DurationUnit::Hours => time::Duration::hours(min_age.amount as i64),
        DurationUnit::Minutes => time::Duration::minutes(min_age.amount as i64),
    };
    (now - published) >= min_duration
}

#[cfg(test)]
mod tests {
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
            Release {
                tag_name: "v5.0.0".into(),
                published_at: "2022-06-01T00:00:00Z".into(),
                prerelease: false,
            },
        ]
    }

    #[test]
    fn minor_bump_stays_on_same_major() {
        let current = Version::new(4, 1, 0);
        let picked = select_release(&releases(), &current, UpdateLevel::Minor, None).unwrap();
        assert_eq!(picked.unwrap().tag_name, "v4.2.0");
    }

    #[test]
    fn major_bump_can_jump_major() {
        let current = Version::new(4, 1, 0);
        let picked = select_release(&releases(), &current, UpdateLevel::Major, None).unwrap();
        assert_eq!(picked.unwrap().tag_name, "v5.0.0");
    }

    #[test]
    fn parse_v_prefix_tag() {
        assert_eq!(parse_semver_tag("v1.2.3"), Some(Version::new(1, 2, 3)));
    }
}
