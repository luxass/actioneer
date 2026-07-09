//! Shared types produced by the scan pipeline.

use std::path::PathBuf;

use serde::Serialize;

use crate::engine::{ActionReference, CommentMatch};
use crate::github::ResolvedRef;

/// A `uses:` reference with its source workflow path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocatedReference {
    /// Workflow path relative to the scan root.
    pub workflow_path: PathBuf,
    /// Parsed reference and its source location.
    pub reference: ActionReference,
}

/// A reference after GitHub resolution and comment matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedReference {
    /// Parsed reference paired with its workflow path.
    pub located: LocatedReference,
    /// Commit resolution for the written ref.
    pub current: ResolvedRef,
    /// Syntactic comparison between the written ref and trailing comment.
    pub comment_match: CommentMatch,
}

/// Audit finding for a single reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AuditIssue {
    /// The reference uses a mutable branch name.
    MutableBranch,
    /// The reference uses an abbreviated commit SHA.
    ShortSha,
    /// The reference is not pinned to a full commit SHA.
    NotShaPinned,
    /// The trailing comment does not describe the written ref.
    CommentMismatch {
        /// Comment text without the leading `#`.
        comment: String,
        /// Ref or version the comment was expected to describe.
        expected: String,
    },
    /// The resolved release has not reached the configured minimum age.
    ReleaseTooYoung {
        /// Configured relative minimum age.
        min_age: String,
        /// RFC 3339 publication time reported by GitHub.
        published_at: String,
    },
    /// Branch processing is disabled by configuration.
    SkippedBranch,
    /// The reference kind is inventoried but is not fully audited or updated.
    SecondaryReference {
        /// Human-readable [`crate::engine::ReferenceKind`] label.
        reference_kind: String,
    },
    /// The current written ref could not be resolved through GitHub.
    ResolutionFailed {
        /// User-facing resolution failure summary.
        message: String,
    },
    /// The written tag floats on a major line such as `v4`.
    FloatingMajorPin {
        /// Floating tag as written in the workflow.
        pin: String,
    },
    /// The pinned commit does not match an official GitHub release.
    UnreleasedCommit {
        /// Full pinned commit SHA.
        sha: String,
    },
    /// A newer release exists but exceeds the configured update level.
    UpdateBlockedByConfig {
        /// Current semantic version label.
        current_version: String,
        /// Newest available semantic version label.
        available_version: String,
        /// Configured update level that rejected the release.
        update_level: String,
    },
    /// A major-only SHA comment does not match the resolved release major.
    CommentMajorLineMismatch {
        /// Major-line comment written in the workflow.
        comment: String,
        /// Full release version inferred from the resolved commit.
        resolved_version: String,
    },
}

/// Why an update was proposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlanReason {
    /// A newer eligible semantic version was selected.
    SemverBump {
        /// Largest semver component changed by the proposal.
        level: String,
    },
    /// The same release should be written with a corrected SHA.
    RePinSha,
    /// A mutable or tag reference should be converted to a SHA pin.
    PinToSha,
    /// The resolved release is current but its written representation is normalized.
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
    /// Full commit SHA resolved for the selected target release.
    pub to_sha: String,
    /// Target semver tag — written as a line comment when pinning to SHA.
    pub to_comment: Option<String>,
    /// Why this change was proposed.
    pub reason: PlanReason,
}

/// Report for a single `uses:` reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceReport {
    /// Parsed reference enriched with GitHub and comment information.
    pub resolved: ResolvedReference,
    /// Audit findings for this reference, in evaluation order.
    pub issues: Vec<AuditIssue>,
    /// Proposed update, or `None` when no safe change is available.
    pub planned: Option<PlannedChange>,
}

/// Report for one workflow file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkflowReport {
    /// Workflow path relative to the scan root.
    pub path: PathBuf,
    /// Top-level workflow `name`, when present.
    pub name: Option<String>,
    /// Reference reports in document order.
    pub references: Vec<ReferenceReport>,
}

/// Aggregate scan statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanStats {
    /// Number of workflow files scanned.
    pub workflows: usize,
    /// Total number of parsed `uses:` references.
    pub references: usize,
    /// References receiving full primary audit rules.
    pub primary: usize,
    /// References inventoried under secondary audit rules.
    pub secondary: usize,
    /// Total number of emitted audit issues.
    pub issues: usize,
    /// References with a proposed update.
    pub planned: usize,
    /// References blocked from planning by release age, skipped branches, or resolution failure.
    pub blocked: usize,
    /// Updates available but excluded by the configured semver level.
    pub config_blocked: usize,
}

/// Full workspace scan result shared by audit and update commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    /// Workflow reports in lexicographic path order.
    pub workflows: Vec<WorkflowReport>,
    /// Aggregate counts for the complete scan.
    pub stats: ScanStats,
}

/// Identifies one planned row to apply (workflow file + source line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyTarget {
    /// Workflow path relative to the apply root.
    pub workflow_path: PathBuf,
    /// One-based source line containing the planned `uses:` reference.
    pub line: u32,
}

/// One successfully applied update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppliedChange {
    /// Workflow path relative to the apply root.
    pub workflow_path: PathBuf,
    /// One-based source line that was, or in dry-run mode would be, rewritten.
    pub line: u32,
    /// Original action reference used to identify the change.
    pub action: String,
    /// Human-readable current pin label.
    pub from: String,
    /// Human-readable target pin label.
    pub to: String,
}

/// A single apply failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyFailure {
    /// Workflow path relative to the apply root.
    pub workflow_path: PathBuf,
    /// One-based source line associated with the failed target.
    pub line: u32,
    /// User-facing reason the target could not be applied.
    pub message: String,
}

/// Result of applying planned updates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyReport {
    /// Changes written successfully, or changes that would be written in dry-run mode.
    pub applied: Vec<AppliedChange>,
    /// Requested targets that could not be verified or written.
    pub failures: Vec<ApplyFailure>,
}

impl ScanReport {
    /// All reference reports across workflows, in document order.
    pub fn all_references(&self) -> impl Iterator<Item = &ReferenceReport> {
        self.workflows.iter().flat_map(|w| w.references.iter())
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
