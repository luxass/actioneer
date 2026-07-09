//! Workflow parsing engine.
//!
//! The engine is responsible for transforming raw GitHub Actions YAML content into
//! structured Rust types. It intentionally does **no** network I/O, no file discovery,
//! and no audit/update logic - those concerns live in higher-level layers.
//!
//! # Entry point
//!
//! ```rust
//! use actioneer::engine::parse_workflow;
//!
//! let doc = parse_workflow(include_str!("../../testdata/workflows/basic.yml")).unwrap();
//! for r in &doc.references {
//!     println!("{} @ {:?}", r.raw, r.pin_kind);
//! }
//! ```
//!
//! # Module layout
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | `mod.rs` (this file) | Public types and re-exports |
//! | `parse` | YAML → [`WorkflowDocument`] |
//! | `reference` | Raw `uses:` string → `ParsedUses` |

mod parse;
mod reference;
mod uses_line;

pub use parse::parse_workflow;
pub use uses_line::{UsesLine, join as join_uses_line, split as split_uses_line, uses_value_start};

use std::fmt;

/// The kind of entity a `uses:` value refers to.
///
/// This classifies **what** is being referenced - separate from how it is pinned
/// ([`PinKind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ReferenceKind {
    /// A GitHub-hosted action: `owner/repo@ref` or `owner/repo/sub/path@ref`.
    Action,
    /// A local action on disk: `./path/to/action` or `../relative/path`.
    LocalAction,
    /// A Docker-based action: `docker://image:tag`.
    Docker,
    /// A reusable workflow: `./.github/workflows/foo.yml@ref`
    /// or `owner/repo/.github/workflows/foo.yml@ref`.
    ReusableWorkflow,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Action => write!(f, "action"),
            Self::LocalAction => write!(f, "local"),
            Self::Docker => write!(f, "docker"),
            Self::ReusableWorkflow => write!(f, "reusable-workflow"),
        }
    }
}

/// How a [`ReferenceKind`] participates in audit and update operations.
///
/// The engine always parses every reference regardless of tier; filtering belongs
/// in the audit/update layer, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AuditTier {
    /// Full pin checks; automatic updates supported (where implemented).
    Primary,
    /// Partial checks and warnings only; no automatic updates in the current iteration.
    Secondary,
}

impl ReferenceKind {
    /// The audit tier for this reference kind.
    ///
    /// | Kind | Tier |
    /// |------|------|
    /// | `Action` | `Primary` |
    /// | `ReusableWorkflow` | `Primary` |
    /// | `Docker` | `Secondary` |
    /// | `LocalAction` | `Secondary` |
    pub fn audit_tier(self) -> AuditTier {
        match self {
            Self::Action | Self::ReusableWorkflow => AuditTier::Primary,
            Self::Docker | Self::LocalAction => AuditTier::Secondary,
        }
    }

    /// Returns `true` if automatic updates are supported for this reference kind.
    ///
    /// | Kind | Updatable |
    /// |------|-----------|
    /// | `Action` | `true` |
    /// | `ReusableWorkflow` | `false` (planned) |
    /// | `Docker` | `false` |
    /// | `LocalAction` | `false` |
    pub fn is_updatable(self) -> bool {
        matches!(self, Self::Action)
    }
}

/// How the `@ref` component of a `uses:` value is pinned.
///
/// Only directly meaningful when the [`ReferenceKind`] is [`ReferenceKind::Action`]
/// or [`ReferenceKind::ReusableWorkflow`]. Docker images and local paths have
/// no `@ref` and will always carry [`PinKind::Unpinned`].
///
/// # SHA threshold
///
/// Full SHA is 40 hex characters (SHA-1). Short SHA is detected as 7–39 all-hex
/// characters. The lower bound of 7 follows `git`'s default abbreviation length.
/// See `docs/engine.md` § "Open questions" for the open debate on this threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PinKind {
    /// Exactly 40 lowercase or uppercase hex characters - a full SHA-1 commit hash.
    FullSha,
    /// 7–39 all-hex characters - a short (abbreviated) SHA.
    ShortSha,
    /// Matches `v\d+.*` - a semver-style tag (`v4`, `v1.2.3`, `v2.0.0-rc1`).
    Tag,
    /// Any other string: a branch name (`main`, `master`, `feature/foo`), `HEAD`, etc.
    Branch,
    /// No `@ref` component at all (local actions, Docker images, or bare `owner/repo`).
    Unpinned,
}

impl fmt::Display for PinKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullSha => write!(f, "full-sha"),
            Self::ShortSha => write!(f, "short-sha"),
            Self::Tag => write!(f, "tag"),
            Self::Branch => write!(f, "branch"),
            Self::Unpinned => write!(f, "unpinned"),
        }
    }
}

/// A single `uses:` reference extracted from a workflow file.
///
/// Carries both the parsed components (owner, repo, ref, ...) and the surrounding
/// context needed for future audit and patching operations (job ID, step index,
/// source line number).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ActionReference {
    /// The raw string exactly as it appears in the `uses:` field.
    pub raw: String,

    /// The kind of entity this reference points to.
    pub kind: ReferenceKind,

    /// How the ref component is pinned.
    pub pin_kind: PinKind,

    /// Action/workflow owner (GitHub user or organisation).
    /// `None` for local paths and Docker images.
    pub owner: Option<String>,

    /// Repository name. `None` for local paths and Docker images.
    pub repo: Option<String>,

    /// Sub-path within the repository - the third `/`-delimited segment and beyond.
    ///
    /// Examples:
    /// - `owner/repo/path/to/action@ref` → `Some("path/to/action")`
    /// - `owner/repo/.github/workflows/deploy.yml@ref` → `Some(".github/workflows/deploy.yml")`
    /// - `./local/action` → `Some("./local/action")`
    /// - `docker://alpine:3` → `Some("alpine:3")`
    pub subpath: Option<String>,

    /// The `@ref` part of the `uses:` value, if present.
    ///
    /// For Docker images this is `None`; the tag is encoded in [`Self::subpath`].
    pub git_ref: Option<String>,

    /// The `name:` field of the step, if present.
    pub step_name: Option<String>,

    /// The job map key (the `jobs.<id>` part).
    pub job_id: String,

    /// The `name:` field of the job, if present.
    pub job_name: Option<String>,

    /// Zero-based index of this step within its job's `steps:` array.
    ///
    /// `None` for job-level `uses:` (reusable-workflow calls at the job level).
    pub step_index: Option<usize>,

    /// 1-based line number in the source file where the `uses:` key appears.
    ///
    /// Determined by a sequential forward scan of the raw content after YAML
    /// deserialization. Accurate as long as the same `uses:` value does not appear
    /// multiple times in the same document; see `docs/engine.md` § "Line tracking".
    pub line: Option<u32>,

    /// Trailing comment on the `uses:` line, if present.
    ///
    /// This is the text after the `#` character on the same line as the `uses:` key,
    /// with leading and trailing whitespace stripped. An empty comment (bare `#`)
    /// is stored as `None`.
    ///
    /// Renovate and similar tools write the human-readable tag here when pinning to
    /// a SHA, e.g. `uses: actions/checkout@deadbeef # v4.2.0` stores `"v4.2.0"`.
    ///
    /// The raw comment text (without `#`) plus the `line` field provide everything
    /// the audit/update layer needs to locate and rewrite the comment in-place.
    pub line_comment: Option<String>,
}

/// A parsed GitHub Actions workflow document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDocument {
    /// The top-level `name:` field, if present.
    pub name: Option<String>,

    /// All `uses:` references extracted from the workflow, in document order.
    ///
    /// Job-level reusable-workflow calls (`jobs.<id>.uses`) appear before
    /// step-level references (`jobs.<id>.steps[*].uses`) within the same job.
    /// Jobs are emitted in the order they appear in the source file.
    pub references: Vec<ActionReference>,
}

/// Result of comparing the trailing comment on a `uses:` line against the pinned ref.
///
/// Used by [`comment_matches_ref`] to express whether a Renovate-style comment is
/// consistent with the `@ref` the action is currently pinned to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CommentMatch {
    /// No trailing comment is present on the `uses:` line.
    NoComment,
    /// The comment corresponds to the pinned ref.
    Match,
    /// A comment is present but does not correspond to the pinned ref.
    Mismatch {
        /// The comment text (without the leading `#`).
        comment: String,
        /// The `git_ref` value that was expected in the comment.
        expected: String,
    },
}

/// Compare the trailing comment on a `uses:` line against the pinned ref.
///
/// This implements Renovate-style comment matching: a SHA-pinned action is expected to
/// carry a human-readable tag in the comment (e.g. `# v4.2.0`), and a tag-pinned action
/// may echo the tag as confirmation (e.g. `@v4 # v4`).
///
/// # Rules
///
/// | Situation | Result |
/// |-----------|--------|
/// | No [`ActionReference::line_comment`] | [`CommentMatch::NoComment`] |
/// | Comment text equals `git_ref` exactly | [`CommentMatch::Match`] |
/// | `git_ref` is a full SHA-1 and the comment contains that SHA | [`CommentMatch::Match`] |
/// | SHA pin with a semver-shaped comment | [`CommentMatch::Match`] (validated by audit) |
/// | Otherwise | [`CommentMatch::Mismatch`] |
///
/// # Examples
///
/// ```rust
/// use actioneer::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind, comment_matches_ref};
///
/// // Tag pin with matching comment → Match
/// let r = ActionReference {
///     raw: "actions/checkout@v4".into(),
///     kind: ReferenceKind::Action,
///     pin_kind: PinKind::Tag,
///     owner: Some("actions".into()),
///     repo: Some("checkout".into()),
///     subpath: None,
///     git_ref: Some("v4".into()),
///     step_name: None,
///     job_id: "build".into(),
///     job_name: None,
///     step_index: Some(0),
///     line: Some(6),
///     line_comment: Some("v4".into()),
/// };
/// assert_eq!(comment_matches_ref(&r), CommentMatch::Match);
/// ```
pub fn comment_matches_ref(reference: &ActionReference) -> CommentMatch {
    let Some(comment) = reference.line_comment.as_deref() else {
        return CommentMatch::NoComment;
    };

    let Some(git_ref) = reference.git_ref.as_deref() else {
        return CommentMatch::Mismatch {
            comment: comment.to_string(),
            expected: String::new(),
        };
    };

    // Exact match: comment text equals git_ref (covers Tag pins like `@v4 # v4`
    // and Branch pins that echo their ref).
    if comment == git_ref {
        return CommentMatch::Match;
    }

    // SHA match: the 40-char git_ref appears verbatim inside the comment.
    // This handles comments that contain the SHA directly, e.g. `# deadbeef...40`.
    if reference.pin_kind == PinKind::FullSha && comment.contains(git_ref) {
        return CommentMatch::Match;
    }

    // SHA pins with semver-shaped comments defer semantic validation to audit.
    if matches!(reference.pin_kind, PinKind::FullSha | PinKind::ShortSha)
        && comment_looks_like_version(comment)
    {
        return CommentMatch::Match;
    }

    CommentMatch::Mismatch {
        comment: comment.to_string(),
        expected: git_ref.to_string(),
    }
}

fn comment_looks_like_version(comment: &str) -> bool {
    let without_v = comment.strip_prefix('v').unwrap_or(comment);
    if !without_v.is_empty() && without_v.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    without_v.contains('.') && semver::Version::parse(without_v).is_ok()
}

/// Errors returned by [`parse_workflow`].
#[derive(Debug)]
pub enum ParseError {
    /// The content could not be parsed as YAML.
    Yaml(serde_yaml::Error),
    /// The document is valid YAML but not a recognisable workflow structure.
    InvalidStructure(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(e) => write!(f, "YAML parse error: {e}"),
            Self::InvalidStructure(msg) => write!(f, "invalid workflow structure: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Yaml(e) => Some(e),
            Self::InvalidStructure(_) => None,
        }
    }
}

impl From<serde_yaml::Error> for ParseError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Yaml(e)
    }
}
