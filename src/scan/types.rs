//! Shared types produced by the scan pipeline.

use std::path::PathBuf;

use serde::Serialize;

use crate::engine::{ActionReference, CommentMatch};
use crate::github::ResolvedRef;

/// A `uses:` reference with its source workflow path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocatedReference {
    pub workflow_path: PathBuf,
    pub reference: ActionReference,
}

/// A reference after GitHub resolution and comment matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedReference {
    pub located: LocatedReference,
    pub current: ResolvedRef,
    pub comment_match: CommentMatch,
}

/// Audit finding for a single reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AuditIssue {
    MutableBranch,
    ShortSha,
    NotShaPinned,
    CommentMismatch {
        comment: String,
        expected: String,
    },
    ReleaseTooYoung {
        min_age: String,
        published_at: String,
    },
    SkippedBranch,
    SecondaryReference {
        reference_kind: String,
    },
    ResolutionFailed {
        message: String,
    },
}

/// Why an update was proposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlanReason {
    SemverBump { level: String },
    RePinSha,
    PinToSha,
    AlreadyUpToDate,
}

/// A proposed update for one reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedChange {
    /// Pin as written in the workflow today (SHA, tag, or branch).
    pub from_ref: String,
    /// Target pin after apply (SHA or tag per `config.pin`).
    pub to_ref: String,
    /// Semver tag (or tag-shaped ref) currently in use, when known.
    pub from_version: Option<String>,
    pub to_sha: String,
    /// Target semver tag — written as a line comment when pinning to SHA.
    pub to_comment: Option<String>,
    pub reason: PlanReason,
}

/// Report for a single `uses:` reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceReport {
    pub resolved: ResolvedReference,
    pub issues: Vec<AuditIssue>,
    pub planned: Option<PlannedChange>,
}

/// Report for one workflow file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkflowReport {
    pub path: PathBuf,
    pub name: Option<String>,
    pub references: Vec<ReferenceReport>,
}

/// Aggregate scan statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanStats {
    pub workflows: usize,
    pub references: usize,
    pub primary: usize,
    pub secondary: usize,
    pub issues: usize,
    pub planned: usize,
    pub blocked: usize,
}

/// Full workspace scan result shared by audit and update commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub workflows: Vec<WorkflowReport>,
    pub stats: ScanStats,
}

/// Identifies one planned row to apply (workflow file + source line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyTarget {
    pub workflow_path: PathBuf,
    pub line: u32,
}

/// One successfully applied update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppliedChange {
    pub workflow_path: PathBuf,
    pub line: u32,
    pub action: String,
    pub from: String,
    pub to: String,
}

/// A single apply failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyFailure {
    pub workflow_path: PathBuf,
    pub line: u32,
    pub message: String,
}

/// Result of applying planned updates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyReport {
    pub applied: Vec<AppliedChange>,
    pub failures: Vec<ApplyFailure>,
}

impl ScanReport {
    /// All reference reports across workflows, in document order.
    pub fn all_references(&self) -> impl Iterator<Item = &ReferenceReport> {
        self.workflows
            .iter()
            .flat_map(|w| w.references.iter())
    }

    /// References with a planned change.
    pub fn planned_changes(&self) -> impl Iterator<Item = (&PathBuf, &ReferenceReport)> {
        self.workflows.iter().flat_map(|w| {
            w.references
                .iter()
                .filter(|r| r.planned.is_some())
                .map(move |r| (&w.path, r))
        })
    }
}
