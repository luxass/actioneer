//! Raw `uses:` string parsing.
//!
//! [`parse_uses`] is the single entry point: it receives the raw string from a
//! `uses:` key and returns a [`ParsedUses`] with all components split out.

use super::{PinKind, ReferenceKind};

/// The result of parsing a single raw `uses:` string.
pub(super) struct ParsedUses {
    pub kind: ReferenceKind,
    pub pin_kind: PinKind,
    /// `None` for local paths and Docker images.
    pub owner: Option<String>,
    /// `None` for local paths and Docker images.
    pub repo: Option<String>,
    /// Sub-path, local path, or Docker image+tag - see [`super::ActionReference::subpath`].
    pub subpath: Option<String>,
    /// The `@ref` component, if present.
    pub git_ref: Option<String>,
}

/// Parse a raw `uses:` string into its constituent parts.
///
/// Recognised forms (in evaluation order):
///
/// 1. `docker://image:tag` - Docker image.
/// 2. `./path` or `../path` - local action or local reusable workflow.
/// 3. `owner/repo[@ref]` - standard action (no sub-path).
/// 4. `owner/repo/sub/path[@ref]` - nested action or reusable workflow.
///
/// Reusable-workflow detection for cases 2 and 4: the path segment after
/// `repo/` contains `.github/workflows/` and ends with `.yml` or `.yaml`.
pub(super) fn parse_uses(raw: &str) -> ParsedUses {
    if let Some(image) = raw.strip_prefix("docker://") {
        return ParsedUses {
            kind: ReferenceKind::Docker,
            pin_kind: PinKind::Unpinned,
            owner: None,
            repo: None,
            subpath: Some(image.to_string()),
            git_ref: None,
        };
    }

    if raw.starts_with("./") || raw.starts_with("../") {
        let (path, git_ref) = split_at_ref(raw);
        let kind = if is_reusable_workflow_path(&path) {
            ReferenceKind::ReusableWorkflow
        } else {
            ReferenceKind::LocalAction
        };
        let pin_kind = git_ref.as_deref().map_or(PinKind::Unpinned, classify_ref);
        return ParsedUses {
            kind,
            pin_kind,
            owner: None,
            repo: None,
            subpath: Some(path),
            git_ref,
        };
    }

    // owner/repo[@ref] or owner/repo/sub/path[@ref]
    let (path_part, git_ref) = split_at_ref(raw);
    let mut segments = path_part.splitn(3, '/');

    let owner = segments.next().unwrap_or("").to_string();
    let Some(repo) = segments.next() else {
        // Malformed - no slash; treat as action with unknown repo.
        return ParsedUses {
            kind: ReferenceKind::Action,
            pin_kind: git_ref.as_deref().map_or(PinKind::Unpinned, classify_ref),
            owner: Some(owner),
            repo: None,
            subpath: None,
            git_ref,
        };
    };
    let repo = repo.to_string();
    let subpath = segments.next().map(str::to_string);

    let kind = if subpath.as_deref().is_some_and(is_reusable_workflow_path) {
        ReferenceKind::ReusableWorkflow
    } else {
        ReferenceKind::Action
    };
    let pin_kind = git_ref.as_deref().map_or(PinKind::Unpinned, classify_ref);

    ParsedUses {
        kind,
        pin_kind,
        owner: Some(owner),
        repo: Some(repo),
        subpath,
        git_ref,
    }
}

/// Split `owner/repo@ref` → `("owner/repo", Some("ref"))`.
///
/// Splits at the first `@`; returns `(full, None)` if absent.
fn split_at_ref(s: &str) -> (String, Option<String>) {
    match s.find('@') {
        Some(i) => (s[..i].to_string(), Some(s[i + 1..].to_string())),
        None => (s.to_string(), None),
    }
}

/// Returns `true` when `path` looks like a reusable-workflow file path.
///
/// Heuristic: the path contains `.github/workflows/` AND ends with `.yml` or `.yaml`.
fn is_reusable_workflow_path(path: &str) -> bool {
    path.contains(".github/workflows/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
}

/// Classify a `@ref` string as [`PinKind::FullSha`], [`PinKind::ShortSha`],
/// [`PinKind::Tag`], or [`PinKind::Branch`].
///
/// Rules:
/// - All-hex, length == 40 → [`PinKind::FullSha`]
/// - All-hex, 7 ≤ length < 40 → [`PinKind::ShortSha`]
/// - Starts with `v` followed by an ASCII digit → [`PinKind::Tag`]
/// - Everything else → [`PinKind::Branch`]
pub(super) fn classify_ref(r: &str) -> PinKind {
    let all_hex = !r.is_empty() && r.chars().all(|c| c.is_ascii_hexdigit());
    if all_hex {
        return match r.len() {
            40 => PinKind::FullSha,
            7..=39 => PinKind::ShortSha,
            _ => PinKind::Branch,
        };
    }
    if r.starts_with('v') && r.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
        return PinKind::Tag;
    }
    PinKind::Branch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_full_sha() {
        assert_eq!(
            classify_ref("a81bbbf8298c0fa03ea29cdc473d45769f953675"),
            PinKind::FullSha
        );
    }

    #[test]
    fn classify_short_sha() {
        assert_eq!(classify_ref("abc1234"), PinKind::ShortSha);
        assert_eq!(classify_ref("abc1234def5678a"), PinKind::ShortSha);
    }

    #[test]
    fn classify_tag_v_prefix() {
        assert_eq!(classify_ref("v4"), PinKind::Tag);
        assert_eq!(classify_ref("v1.2.3"), PinKind::Tag);
        assert_eq!(classify_ref("v2.0.0-rc1"), PinKind::Tag);
    }

    #[test]
    fn classify_branch() {
        assert_eq!(classify_ref("main"), PinKind::Branch);
        assert_eq!(classify_ref("master"), PinKind::Branch);
        assert_eq!(classify_ref("feature/my-feature"), PinKind::Branch);
        assert_eq!(classify_ref("HEAD"), PinKind::Branch);
    }

    #[test]
    fn parse_standard_action_tag() {
        let p = parse_uses("actions/checkout@v4");
        assert_eq!(p.kind, ReferenceKind::Action);
        assert_eq!(p.pin_kind, PinKind::Tag);
        assert_eq!(p.owner.as_deref(), Some("actions"));
        assert_eq!(p.repo.as_deref(), Some("checkout"));
        assert_eq!(p.git_ref.as_deref(), Some("v4"));
        assert!(p.subpath.is_none());
    }

    #[test]
    fn parse_action_full_sha() {
        let p = parse_uses("actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675");
        assert_eq!(p.pin_kind, PinKind::FullSha);
        assert_eq!(
            p.git_ref.as_deref(),
            Some("a81bbbf8298c0fa03ea29cdc473d45769f953675")
        );
    }

    #[test]
    fn parse_nested_action() {
        let p = parse_uses("actions/aws-actions/amazon-ecr-login@v2");
        assert_eq!(p.kind, ReferenceKind::Action);
        assert_eq!(p.owner.as_deref(), Some("actions"));
        assert_eq!(p.repo.as_deref(), Some("aws-actions"));
        assert_eq!(p.subpath.as_deref(), Some("amazon-ecr-login"));
        assert_eq!(p.git_ref.as_deref(), Some("v2"));
    }

    #[test]
    fn parse_local_action() {
        let p = parse_uses("./my-local-action");
        assert_eq!(p.kind, ReferenceKind::LocalAction);
        assert_eq!(p.pin_kind, PinKind::Unpinned);
        assert!(p.owner.is_none());
        assert!(p.repo.is_none());
        assert_eq!(p.subpath.as_deref(), Some("./my-local-action"));
    }

    #[test]
    fn parse_local_reusable_workflow() {
        let p = parse_uses("./.github/workflows/deploy.yml");
        assert_eq!(p.kind, ReferenceKind::ReusableWorkflow);
        assert_eq!(p.pin_kind, PinKind::Unpinned);
    }

    #[test]
    fn parse_local_reusable_workflow_with_ref() {
        let p = parse_uses("./.github/workflows/deploy.yml@main");
        assert_eq!(p.kind, ReferenceKind::ReusableWorkflow);
        assert_eq!(p.pin_kind, PinKind::Branch);
        assert_eq!(p.git_ref.as_deref(), Some("main"));
    }

    #[test]
    fn parse_remote_reusable_workflow() {
        let p = parse_uses("octo-org/octo-repo/.github/workflows/workflow.yml@v1");
        assert_eq!(p.kind, ReferenceKind::ReusableWorkflow);
        assert_eq!(p.owner.as_deref(), Some("octo-org"));
        assert_eq!(p.repo.as_deref(), Some("octo-repo"));
        assert_eq!(p.subpath.as_deref(), Some(".github/workflows/workflow.yml"));
        assert_eq!(p.pin_kind, PinKind::Tag);
    }

    #[test]
    fn parse_docker_image() {
        let p = parse_uses("docker://alpine:3.14");
        assert_eq!(p.kind, ReferenceKind::Docker);
        assert_eq!(p.pin_kind, PinKind::Unpinned);
        assert_eq!(p.subpath.as_deref(), Some("alpine:3.14"));
        assert!(p.git_ref.is_none());
    }
}
