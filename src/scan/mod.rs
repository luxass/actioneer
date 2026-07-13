//! Workspace scan pipeline shared by audit and update commands.

mod apply;
mod audit;
mod display;
mod pin;
mod plan;
mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fmt, io};

use crate::config::ActioneerConfig;
use crate::discovery::{self, DiscoveryError};
use crate::engine::{ParseError, PinKind, comment_matches_ref, parse_workflow};
use crate::github::{GitHubClient, GitHubError, Release, ResolvedRef};

use pin::{TagShape, classify_tag};

pub use types::{
    AppliedChange, ApplyFailure, ApplyReport, ApplyTarget, AuditIssue, LocatedReference,
    PlanReason, PlannedChange, ReferenceReport, ResolvedReference, ScanReport, ScanStats,
    WorkflowReport,
};

pub use apply::{all_planned_targets, apply};
pub use display::{plan_from_label, plan_to_label, truncate_label};

/// Errors during workspace scanning.
#[derive(Debug)]
pub enum ScanError {
    /// Workflow target discovery failed.
    Discovery(DiscoveryError),
    /// A workflow or apply target could not be read or written.
    Io(io::Error),
    /// A discovered workflow could not be parsed.
    Parse {
        /// Workflow path relative to the scan root.
        path: PathBuf,
        /// Parser error returned by the engine.
        error: ParseError,
    },
    /// A GitHub operation required for a complete scan failed.
    GitHub(GitHubError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse { path, error } => write!(f, "{}: {error}", path.display()),
            Self::GitHub(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Discovery(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Parse { error, .. } => Some(error),
            Self::GitHub(e) => Some(e),
        }
    }
}

impl From<DiscoveryError> for ScanError {
    fn from(e: DiscoveryError) -> Self {
        Self::Discovery(e)
    }
}

impl From<io::Error> for ScanError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<GitHubError> for ScanError {
    fn from(e: GitHubError) -> Self {
        Self::GitHub(e)
    }
}

/// Discover workflows, resolve references, audit, and plan updates.
///
/// GitHub API calls per unique remote action:
/// - one `list_releases` per owner/repo
/// - one `resolve_ref` per unique pin (cached)
/// - at most one `resolve_ref` for the planned target tag
/// - for SHA pins with semver comments: one `resolve_ref` for that comment tag
///
/// Paths in the returned report are relative to `root`. Passing no
/// `workflow_paths` discovers the flat `root/.github/workflows` directory.
///
/// # Errors
///
/// Discovery, file reads, parsing, release-list requests, and GitHub requests
/// required to construct a plan abort the scan. A failure to resolve the current
/// written ref is instead represented by [`AuditIssue::ResolutionFailed`], and
/// semver-comment resolution is best-effort.
///
/// # Side effects
///
/// The function reads workflow files and may read or populate the GitHub cache
/// or perform network requests according to `config` and `client`. It does not
/// modify workflow files.
pub fn scan_workspace(
    root: &Path,
    workflow_paths: &[PathBuf],
    config: &ActioneerConfig,
    client: &GitHubClient,
) -> Result<ScanReport, ScanError> {
    let paths = discovery::resolve_workflow_paths(root, workflow_paths)?;
    let mut workflows = Vec::new();
    let mut stats = ScanStats::default();

    let mut resolve_cache: HashMap<(String, String, String), ResolvedRef> = HashMap::new();
    let mut releases_cache: HashMap<(String, String), Vec<Release>> = HashMap::new();
    let mut sha_version_cache: HashMap<(String, String), HashMap<String, Option<semver::Version>>> =
        HashMap::new();

    for path in paths {
        let file_path = root.join(&path);
        let content = std::fs::read_to_string(&file_path)?;
        let document = parse_workflow(&content).map_err(|error| ScanError::Parse {
            path: path.clone(),
            error,
        })?;

        let mut references = Vec::new();

        for reference in document.references {
            stats.references += 1;
            if reference.kind.audit_tier() == crate::engine::AuditTier::Primary
                && !reference.is_local_reusable_workflow()
            {
                stats.primary += 1;
            } else {
                stats.secondary += 1;
            }

            let located = LocatedReference {
                workflow_path: path.clone(),
                reference: reference.clone(),
            };

            let releases = if reference.kind.audit_tier() == crate::engine::AuditTier::Primary
                && !reference.is_local_reusable_workflow()
            {
                if let (Some(owner), Some(repo)) = (&reference.owner, &reference.repo) {
                    fetch_releases(client, owner, repo, &mut releases_cache)?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let (mut resolved, resolution_failed) =
                resolve_reference(&located, client, &mut resolve_cache)?;

            enrich_published_at(&mut resolved, &releases);

            let comment_tag_sha =
                if let (Some(owner), Some(repo)) = (&reference.owner, &reference.repo) {
                    resolve_comment_tag_sha(&reference, client, owner, repo, &mut resolve_cache)?
                } else {
                    None
                };

            let mut issues = audit::evaluate(
                &resolved,
                &releases,
                config,
                comment_tag_sha
                    .as_ref()
                    .map(|(tag, sha)| (tag.as_str(), sha.as_str())),
            );

            if resolution_failed {
                issues.push(AuditIssue::ResolutionFailed {
                    message: "failed to resolve reference on GitHub".into(),
                });
            }
            stats.issues += issues.len();
            stats.config_blocked += issues
                .iter()
                .filter(|i| matches!(i, AuditIssue::UpdateBlockedByConfig { .. }))
                .count();

            let planned = if !reference.is_local_reusable_workflow()
                && let (Some(owner), Some(repo)) = (&reference.owner, &reference.repo)
            {
                let repo_sha_cache = sha_version_cache
                    .entry((owner.clone(), repo.clone()))
                    .or_default();
                let mut plan_ctx = plan::PlanContext {
                    client,
                    owner,
                    repo,
                    resolve_cache: &mut resolve_cache,
                    sha_version_cache: repo_sha_cache,
                };
                plan::propose(&resolved, &releases, config, &mut plan_ctx, &issues)?
            } else {
                None
            };

            if planned.is_some() {
                stats.planned += 1;
            } else if reference.kind.is_updatable()
                && !issues.is_empty()
                && audit::blocks_update(&issues)
            {
                stats.blocked += 1;
            }

            references.push(ReferenceReport {
                resolved,
                issues,
                planned,
            });
        }

        stats.workflows += 1;
        workflows.push(WorkflowReport {
            path,
            name: document.name,
            references,
        });
    }

    Ok(ScanReport { workflows, stats })
}

fn enrich_published_at(resolved: &mut ResolvedReference, releases: &[Release]) {
    if resolved.current.published_at.is_some() {
        return;
    }
    let Some(git_ref) = resolved.located.reference.git_ref.as_deref() else {
        return;
    };
    resolved.current.published_at = releases
        .iter()
        .find(|r| r.tag_name == git_ref)
        .map(|r| r.published_at.clone());
}

fn resolve_reference(
    located: &LocatedReference,
    client: &GitHubClient,
    cache: &mut HashMap<(String, String, String), ResolvedRef>,
) -> Result<(ResolvedReference, bool), ScanError> {
    let reference = &located.reference;
    let comment_match = comment_matches_ref(reference);

    if reference.is_local_reusable_workflow()
        || reference.kind.audit_tier() == crate::engine::AuditTier::Secondary
    {
        let placeholder = ResolvedRef {
            sha: String::new(),
            ref_kind: crate::github::RefKind::Sha,
            published_at: None,
        };
        return Ok((
            ResolvedReference {
                located: located.clone(),
                current: placeholder,
                comment_match,
            },
            false,
        ));
    }

    let (owner, repo, git_ref) = match (
        reference.owner.as_deref(),
        reference.repo.as_deref(),
        reference.git_ref.as_deref(),
    ) {
        (Some(o), Some(r), Some(g)) => (o, r, g),
        _ => {
            let placeholder = ResolvedRef {
                sha: String::new(),
                ref_kind: crate::github::RefKind::Sha,
                published_at: None,
            };
            return Ok((
                ResolvedReference {
                    located: located.clone(),
                    current: placeholder,
                    comment_match,
                },
                true,
            ));
        }
    };

    match cached_resolve(client, owner, repo, git_ref, cache) {
        Ok(current) => Ok((
            ResolvedReference {
                located: located.clone(),
                current,
                comment_match,
            },
            false,
        )),
        Err(_) => {
            let placeholder = ResolvedRef {
                sha: String::new(),
                ref_kind: crate::github::RefKind::Sha,
                published_at: None,
            };
            Ok((
                ResolvedReference {
                    located: located.clone(),
                    current: placeholder,
                    comment_match,
                },
                true,
            ))
        }
    }
}

/// For SHA pins with a semver comment, resolve that tag once to verify the official release SHA.
fn resolve_comment_tag_sha(
    reference: &crate::engine::ActionReference,
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    cache: &mut HashMap<(String, String, String), ResolvedRef>,
) -> Result<Option<(String, String)>, ScanError> {
    if !matches!(reference.pin_kind, PinKind::FullSha | PinKind::ShortSha) {
        return Ok(None);
    }
    let Some(comment) = reference.line_comment.as_deref() else {
        return Ok(None);
    };
    if !matches!(
        classify_tag(comment),
        TagShape::FullSemver(_) | TagShape::MajorOnly(_)
    ) {
        return Ok(None);
    }

    let resolved = cached_resolve(client, owner, repo, comment, cache);
    Ok(resolved.ok().map(|r| (comment.to_string(), r.sha)))
}

fn cached_resolve(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    git_ref: &str,
    cache: &mut HashMap<(String, String, String), ResolvedRef>,
) -> Result<ResolvedRef, GitHubError> {
    let key = (owner.to_string(), repo.to_string(), git_ref.to_string());
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let resolved = client.resolve_ref(owner, repo, git_ref)?;
    cache.insert(key, resolved.clone());
    Ok(resolved)
}

fn fetch_releases(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
    cache: &mut HashMap<(String, String), Vec<Release>>,
) -> Result<Vec<Release>, ScanError> {
    let key = (owner.to_string(), repo.to_string());
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let releases = client.list_releases(owner, repo)?;
    cache.insert(key, releases.clone());
    Ok(releases)
}
