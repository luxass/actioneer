//! Update planning from GitHub releases and config.

use std::collections::HashMap;

use semver::Version;
use time::format_description::well_known::Rfc3339;

use crate::config::{ActioneerConfig, DurationUnit, PinMode, RelativeDuration, UpdateLevel};
use crate::engine::PinKind;
use crate::github::{GitHubClient, GitHubError, Release, ResolvedRef};

use super::audit::blocks_update;
use super::pin::{
    VersionBaseline, latest_on_major, parse_semver_tag, version_baseline, would_change,
    written_version_tag,
};
use super::types::{AuditIssue, PlanReason, PlannedChange, ResolvedReference};

/// GitHub resolution context shared across plan steps.
pub struct PlanContext<'a> {
    pub client: &'a GitHubClient,
    pub owner: &'a str,
    pub repo: &'a str,
    pub resolve_cache: &'a mut HashMap<(String, String, String), ResolvedRef>,
    /// Maps commit SHA → inferred semver on the current major line (per scan).
    pub sha_version_cache: &'a mut HashMap<String, Option<Version>>,
}

/// Propose an update for one reference, if applicable.
///
/// GitHub calls: at most one `resolve_ref` for the chosen target tag (cached).
/// Current pin is already resolved by the scan pipeline.
pub fn propose(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    ctx: &mut PlanContext<'_>,
    issues: &[AuditIssue],
) -> Result<Option<PlannedChange>, GitHubError> {
    let reference = &resolved.located.reference;

    if !reference.kind.is_updatable() || blocks_update(issues) {
        return Ok(None);
    }

    if reference.pin_kind == PinKind::Branch && config.skip_branches {
        return Ok(None);
    }

    let current_sha = &resolved.current.sha;
    if current_sha.is_empty() {
        return Ok(None);
    }

    if let Some(comment) = issues.iter().find_map(|issue| match issue {
        AuditIssue::ShaCommentMismatch { comment, .. } => Some(comment.as_str()),
        _ => None,
    }) {
        return propose_comment_mismatch_remediation(resolved, releases, config, ctx, comment);
    }

    let written_tag = written_version_tag(reference);
    let from_version = written_tag.clone();

    if let VersionBaseline::MajorOnly(major) = version_baseline(reference) {
        return propose_major_only(
            resolved,
            releases,
            config,
            ctx,
            major,
            current_sha,
            from_version,
        );
    }

    let current_ver = match version_baseline(reference) {
        VersionBaseline::Exact(v) => v,
        VersionBaseline::Unknown => return Ok(None),
        VersionBaseline::MajorOnly(_) => unreachable!("handled above"),
    };

    let candidate = select_release(
        releases,
        &current_ver,
        config.update,
        config.min_release_age,
    )?;
    let Some(release) = candidate else {
        return Ok(None);
    };

    if release.tag_name == written_tag.as_deref().unwrap_or("") {
        return Ok(None);
    }

    build_plan(resolved, &release, from_version, config, ctx)
}

fn propose_major_only(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    ctx: &mut PlanContext<'_>,
    major: u64,
    current_sha: &str,
    from_version: Option<String>,
) -> Result<Option<PlannedChange>, GitHubError> {
    let Some(latest) = latest_on_major_with_age(releases, major, config.min_release_age) else {
        return Ok(None);
    };

    let latest_resolved = match cached_resolve(ctx, &latest.tag_name) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    if current_sha == latest_resolved.sha {
        return build_plan(resolved, latest, from_version, config, ctx);
    }

    let current_ver = infer_version_on_major(current_sha, releases, major, ctx)?
        .unwrap_or_else(|| Version::new(major, 0, 0));

    let filtered = filter_releases_on_major(releases, major);
    let Some(release) = select_release(
        &filtered,
        &current_ver,
        config.update,
        config.min_release_age,
    )?
    else {
        return Ok(None);
    };

    build_plan(resolved, &release, from_version, config, ctx)
}

fn propose_comment_mismatch_remediation(
    resolved: &ResolvedReference,
    releases: &[Release],
    config: &ActioneerConfig,
    ctx: &mut PlanContext<'_>,
    comment: &str,
) -> Result<Option<PlannedChange>, GitHubError> {
    let now = time::OffsetDateTime::now_utc();
    let Some(release) = releases.iter().find(|release| {
        release.tag_name == comment
            && !release.prerelease
            && release_meets_min_age(release, config.min_release_age, now)
    }) else {
        return Ok(None);
    };

    build_plan(resolved, release, Some(comment.to_string()), config, ctx)
}

fn build_plan(
    resolved: &ResolvedReference,
    release: &Release,
    from_version: Option<String>,
    config: &ActioneerConfig,
    ctx: &mut PlanContext<'_>,
) -> Result<Option<PlannedChange>, GitHubError> {
    let reference = &resolved.located.reference;
    let target_resolved = match cached_resolve(ctx, &release.tag_name) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let (to_ref, to_comment) = match config.pin {
        PinMode::Sha => (target_resolved.sha.clone(), Some(release.tag_name.clone())),
        PinMode::Tag => (release.tag_name.clone(), None),
    };

    let from_ref = reference
        .git_ref
        .clone()
        .unwrap_or_else(|| reference.raw.clone());

    let planned = PlannedChange {
        from_ref,
        to_ref,
        from_version,
        to_sha: target_resolved.sha,
        to_comment,
        reason: PlanReason::SemverBump {
            level: config.update.to_string(),
        },
    };

    if !would_change(reference, &planned, config) {
        return Ok(None);
    }

    Ok(Some(planned))
}

/// When `@v4` lags behind, infer effective semver by walking releases newest-first.
/// Stops at the first tag whose SHA matches. Populates [`PlanContext::sha_version_cache`]
/// for every tag resolved along the walk.
fn infer_version_on_major(
    sha: &str,
    releases: &[Release],
    major: u64,
    ctx: &mut PlanContext<'_>,
) -> Result<Option<Version>, GitHubError> {
    if let Some(cached) = ctx.sha_version_cache.get(sha) {
        return Ok(cached.clone());
    }

    let mut sorted: Vec<&Release> = releases
        .iter()
        .filter(|r| !r.prerelease)
        .filter(|r| parse_semver_tag(&r.tag_name).is_some_and(|v| v.major == major))
        .collect();
    sorted.sort_by(|a, b| {
        let va = parse_semver_tag(&a.tag_name).unwrap();
        let vb = parse_semver_tag(&b.tag_name).unwrap();
        vb.cmp(&va)
    });

    for release in sorted {
        let Ok(resolved) = cached_resolve(ctx, &release.tag_name) else {
            continue;
        };
        let version = parse_semver_tag(&release.tag_name);
        ctx.sha_version_cache
            .insert(resolved.sha.clone(), version.clone());
        if resolved.sha == sha {
            return Ok(version);
        }
    }
    ctx.sha_version_cache.insert(sha.to_string(), None);
    Ok(None)
}

fn cached_resolve(ctx: &mut PlanContext<'_>, git_ref: &str) -> Result<ResolvedRef, GitHubError> {
    let key = (
        ctx.owner.to_string(),
        ctx.repo.to_string(),
        git_ref.to_string(),
    );
    if let Some(cached) = ctx.resolve_cache.get(&key) {
        return Ok(cached.clone());
    }
    let resolved = ctx.client.resolve_ref(ctx.owner, ctx.repo, git_ref)?;
    ctx.resolve_cache.insert(key, resolved.clone());
    Ok(resolved)
}

fn filter_releases_on_major(releases: &[Release], major: u64) -> Vec<Release> {
    releases
        .iter()
        .filter(|r| {
            !r.prerelease && parse_semver_tag(&r.tag_name).is_some_and(|v| v.major == major)
        })
        .cloned()
        .collect()
}

fn latest_on_major_with_age(
    releases: &[Release],
    major: u64,
    min_age: Option<RelativeDuration>,
) -> Option<&Release> {
    let now = time::OffsetDateTime::now_utc();
    latest_on_major(releases, major).filter(|r| release_meets_min_age(r, min_age, now))
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
        .filter(|r| parse_semver_tag(&r.tag_name).is_some_and(|v| is_candidate(current, &v, level)))
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
        UpdateLevel::Patch => candidate.major == current.major && candidate.minor == current.minor,
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
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::cache::resolve_cache_dir_with;
    use crate::config::{ActioneerConfig, PinMode, UpdateLevel};
    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{CacheEntry, GitHubClient, RefKind, ResolvedRef};
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

    fn sha_b() -> String {
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()
    }

    fn seed_tag_cache(dir: &TempDir, tag: &str, sha: &str) {
        let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
        let path = cache
            .path()
            .join("github/actions/checkout/refs/tags")
            .join(format!("{tag}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let entry = CacheEntry {
            sha: sha.to_string(),
            ref_kind: "tag".into(),
            published_at: Some("2020-01-01T00:00:00Z".into()),
            fetched_at: 1_700_000_000,
        };
        fs::write(&path, serde_json::to_vec_pretty(&entry).unwrap()).unwrap();
    }

    fn offline_client(dir: &TempDir) -> GitHubClient {
        let config = ActioneerConfig {
            offline: true,
            ..Default::default()
        };
        GitHubClient::new(
            &config,
            resolve_cache_dir_with(Some(dir.path().to_str().unwrap())),
        )
    }

    fn tag_ref(git_ref: &str) -> ResolvedReference {
        ResolvedReference {
            located: LocatedReference {
                workflow_path: PathBuf::from(".github/workflows/ci.yml"),
                reference: ActionReference {
                    raw: format!("actions/checkout@{git_ref}"),
                    kind: ReferenceKind::Action,
                    pin_kind: PinKind::Tag,
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
                sha: sha_b(),
                ref_kind: RefKind::Tag,
                published_at: Some("2020-01-01T00:00:00Z".into()),
            },
            comment_match: CommentMatch::NoComment,
        }
    }

    fn sha_ref(sha: &str, comment: &str) -> ResolvedReference {
        let mut resolved = tag_ref(sha);
        resolved.located.reference.raw = format!("actions/checkout@{sha} # {comment}");
        resolved.located.reference.pin_kind = PinKind::FullSha;
        resolved.located.reference.line_comment = Some(comment.into());
        resolved.current.sha = sha.into();
        resolved.current.ref_kind = RefKind::Sha;
        resolved
    }

    #[test]
    fn major_only_plans_to_latest_on_line() {
        let dir = TempDir::new().unwrap();
        seed_tag_cache(&dir, "v4.2.0", &sha_b());

        let config = ActioneerConfig {
            pin: PinMode::Tag,
            update: UpdateLevel::Minor,
            offline: true,
            ..Default::default()
        };
        let client = offline_client(&dir);
        let resolved = tag_ref("v4");
        let mut cache = HashMap::new();
        let mut sha_cache = HashMap::new();
        let mut ctx = PlanContext {
            client: &client,
            owner: "actions",
            repo: "checkout",
            resolve_cache: &mut cache,
            sha_version_cache: &mut sha_cache,
        };

        let planned = propose(&resolved, &releases(), &config, &mut ctx, &[])
            .unwrap()
            .unwrap();

        assert_eq!(planned.from_version.as_deref(), Some("v4"));
        assert_eq!(planned.to_ref, "v4.2.0");
    }

    #[test]
    fn major_only_same_sha_still_plans_normalization() {
        let dir = TempDir::new().unwrap();
        seed_tag_cache(&dir, "v4.2.0", &sha_b());

        let config = ActioneerConfig {
            pin: PinMode::Tag,
            update: UpdateLevel::Minor,
            offline: true,
            ..Default::default()
        };
        let client = offline_client(&dir);
        let resolved = tag_ref("v4");
        let mut cache = HashMap::new();
        let mut sha_cache = HashMap::new();
        let mut ctx = PlanContext {
            client: &client,
            owner: "actions",
            repo: "checkout",
            resolve_cache: &mut cache,
            sha_version_cache: &mut sha_cache,
        };

        assert!(
            propose(&resolved, &releases(), &config, &mut ctx, &[])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn mismatch_remediation_respects_release_safety_filters() {
        let dir = TempDir::new().unwrap();
        seed_tag_cache(&dir, "v4.2.0", &sha_b());

        let config = ActioneerConfig {
            offline: true,
            min_release_age: Some(RelativeDuration {
                amount: 1,
                unit: DurationUnit::Days,
            }),
            ..Default::default()
        };
        let client = offline_client(&dir);
        let resolved = sha_ref("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "v4.2.0");
        let issues = vec![AuditIssue::ShaCommentMismatch {
            comment: "v4.2.0".into(),
            expected_sha: sha_b(),
        }];
        let mut cache = HashMap::new();
        let mut sha_cache = HashMap::new();
        let mut ctx = PlanContext {
            client: &client,
            owner: "actions",
            repo: "checkout",
            resolve_cache: &mut cache,
            sha_version_cache: &mut sha_cache,
        };

        let too_young = Release {
            tag_name: "v4.2.0".into(),
            published_at: "2999-01-01T00:00:00Z".into(),
            prerelease: false,
        };
        assert!(
            propose(&resolved, &[too_young], &config, &mut ctx, &issues)
                .unwrap()
                .is_none()
        );

        let prerelease = Release {
            tag_name: "v4.2.0".into(),
            published_at: "2020-01-01T00:00:00Z".into(),
            prerelease: true,
        };
        assert!(
            propose(&resolved, &[prerelease], &config, &mut ctx, &issues)
                .unwrap()
                .is_none()
        );
    }
}
