//! Workspace scan pipeline shared by audit and update commands.

mod apply;
mod audit;
mod display;
mod plan;
pub mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fmt, io};

use crate::config::ActioneerConfig;
use crate::discovery::{self, DiscoveryError};
use crate::engine::{comment_matches_ref, parse_workflow, ParseError};
use crate::github::{GitHubClient, GitHubError, Release, ResolvedRef};

pub use types::{
    AppliedChange, ApplyFailure, ApplyReport, ApplyTarget, AuditIssue, LocatedReference,
    PlannedChange, PlanReason, ReferenceReport, ResolvedReference, ScanReport, ScanStats,
    WorkflowReport,
};

pub use apply::{all_planned_targets, apply};
pub use display::{plan_from_label, plan_to_label, truncate_label};

/// Errors during workspace scanning.
#[derive(Debug)]
pub enum ScanError {
    Discovery(DiscoveryError),
    Io(io::Error),
    Parse { path: PathBuf, error: ParseError },
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
pub fn scan_workspace(
    root: &Path,
    config: &ActioneerConfig,
    client: &GitHubClient,
) -> Result<ScanReport, ScanError> {
    let paths = discovery::discover_workflows(root)?;
    let mut workflows = Vec::new();
    let mut stats = ScanStats::default();

    let mut resolve_cache: HashMap<(String, String, String), ResolvedRef> = HashMap::new();
    let mut releases_cache: HashMap<(String, String), Vec<Release>> = HashMap::new();

    for path in paths {
        let content = std::fs::read_to_string(&path)?;
        let document = parse_workflow(&content).map_err(|error| ScanError::Parse {
            path: path.clone(),
            error,
        })?;

        let mut references = Vec::new();

        for reference in document.references {
            stats.references += 1;
            if reference.kind.audit_tier() == crate::engine::AuditTier::Primary {
                stats.primary += 1;
            } else {
                stats.secondary += 1;
            }

            let located = LocatedReference {
                workflow_path: path.clone(),
                reference: reference.clone(),
            };

            let (resolved, resolution_failed) =
                resolve_reference(&located, client, &mut resolve_cache)?;

            let mut issues = audit::evaluate(&resolved, config);
            if resolution_failed {
                issues.push(AuditIssue::ResolutionFailed {
                    message: "failed to resolve reference on GitHub".into(),
                });
            }
            stats.issues += issues.len();

            let releases_vec = if reference.kind.is_updatable() {
                if let (Some(owner), Some(repo)) = (&reference.owner, &reference.repo) {
                    fetch_releases(client, owner, repo, &mut releases_cache)?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let planned = plan::propose(&resolved, &releases_vec, config, client, &issues)?;
            if planned.is_some() {
                stats.planned += 1;
            } else if reference.kind.is_updatable() && !issues.is_empty() && audit::blocks_update(&issues) {
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

fn resolve_reference(
    located: &LocatedReference,
    client: &GitHubClient,
    cache: &mut HashMap<(String, String, String), ResolvedRef>,
) -> Result<(ResolvedReference, bool), ScanError> {
    let reference = &located.reference;
    let comment_match = comment_matches_ref(reference);

    if reference.kind.audit_tier() == crate::engine::AuditTier::Secondary {
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

    let key = (owner.to_string(), repo.to_string(), git_ref.to_string());
    if let Some(cached) = cache.get(&key) {
        return Ok((
            ResolvedReference {
                located: located.clone(),
                current: cached.clone(),
                comment_match,
            },
            false,
        ));
    }

    match client.resolve_ref(owner, repo, git_ref) {
        Ok(current) => {
            cache.insert(key, current.clone());
            Ok((
                ResolvedReference {
                    located: located.clone(),
                    current,
                    comment_match,
                },
                false,
            ))
        }
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
