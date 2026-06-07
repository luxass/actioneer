use std::collections::HashMap;

use crate::actions::reference::{ActionReference, ActionUpdate};
use crate::actions::version::{Version, is_likely_sha, parse_version, sha_matches};

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub sha: String,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum PinStyle {
    #[default]
    Sha,
    Tag,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum UpdateMode {
    #[default]
    Major,
    Minor,
    Patch,
}

#[derive(Debug, Clone)]
pub struct ResolveConfig {
    pub excludes: Vec<String>,
    pub skip_branches: bool,
    pub mode: UpdateMode,
    pub style: PinStyle,
}

#[derive(Clone, Copy, Debug)]
enum CurrentRefKind {
    Version,
    Sha,
    Branch,
}

pub fn resolve(
    actions: &[ActionReference],
    tags: &HashMap<(String, String), Vec<Tag>>,
    config: &ResolveConfig,
) -> Vec<ActionUpdate> {
    let mut updates = Vec::new();
    for action in actions {
        let action_name = action.action_name();
        if config.excludes.iter().any(|ex| action_name.contains(ex)) {
            continue;
        }

        let Some(ctx) = classify(action, config.skip_branches) else {
            continue;
        };

        let Some(repo_tags) = tags.get(&(action.owner.clone(), action.name.clone())) else {
            continue;
        };
        if repo_tags.is_empty() {
            continue;
        }

        let comment_tag = action
            .version_comment
            .as_deref()
            .and_then(|vc| repo_tags.iter().find(|t| t.name == vc));

        let expected_sha = if let Some(tag) = comment_tag {
            tag.sha.clone()
        } else if let Some(tag_ref) = repo_tags.iter().find(|t| {
            t.name == action.current_ref
                || t.sha == action.current_ref
                || t.sha.starts_with(&action.current_ref)
        }) {
            tag_ref.sha.clone()
        } else if matches!(ctx, CurrentRefKind::Sha) {
            action.current_ref.clone()
        } else {
            String::new()
        };

        let sha_mismatch = matches!(ctx, CurrentRefKind::Sha)
            && comment_tag
                .map(|t| !sha_matches(&action.current_ref, &t.sha))
                .unwrap_or(false);

        let current_version = match ctx {
            CurrentRefKind::Version => parse_version(&action.current_ref),
            CurrentRefKind::Sha => action.version_comment.as_deref().and_then(parse_version),
            CurrentRefKind::Branch => None,
        };
        let Some(target) = best_target(repo_tags, current_version, config.mode) else {
            continue;
        };

        if let Some(cur) = current_version
            && target.version < cur
        {
            continue;
        }

        if !sha_mismatch && current_ref_matches_style(&action.current_ref, target, config.style) {
            continue;
        }

        updates.push(ActionUpdate {
            action: action.clone(),
            new_ref: match config.style {
                PinStyle::Sha => target.sha.clone(),
                PinStyle::Tag => target.name.clone(),
            },
            new_version: target.name.clone(),
            expected_sha,
            sha_mismatch,
            is_branch: matches!(ctx, CurrentRefKind::Branch),
            is_major: current_version
                .map(|v| target.version.major > v.major)
                .unwrap_or(false),
        });
    }
    updates
}

fn classify(action: &ActionReference, skip_branches: bool) -> Option<CurrentRefKind> {
    if is_likely_sha(&action.current_ref)
        || (action.version_comment.is_some() && is_sha_like_ref(&action.current_ref))
    {
        return Some(CurrentRefKind::Sha);
    }
    if parse_version(&action.current_ref).is_some() {
        return Some(CurrentRefKind::Version);
    }
    if skip_branches {
        return None;
    }
    Some(CurrentRefKind::Branch)
}

fn is_sha_like_ref(value: &str) -> bool {
    value.len() >= 7 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn best_target(tags: &[Tag], current: Option<Version>, mode: UpdateMode) -> Option<&Tag> {
    tags.iter()
        .filter(|t| match current {
            Some(v) => match mode {
                UpdateMode::Minor => t.version.major == v.major,
                UpdateMode::Patch => t.version.major == v.major && t.version.minor == v.minor,
                UpdateMode::Major => true,
            },
            None => true,
        })
        .max_by_key(|t| t.version)
}

fn current_ref_matches_style(current_ref: &str, target: &Tag, style: PinStyle) -> bool {
    match style {
        PinStyle::Tag => current_ref == target.name,
        PinStyle::Sha => sha_matches(current_ref, &target.sha),
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::version::Version;

    use super::*;

    fn action(owner: &str, name: &str, current_ref: &str, vc: Option<&str>) -> ActionReference {
        ActionReference {
            owner: owner.to_string(),
            name: name.to_string(),
            path: String::new(),
            current_ref: current_ref.to_string(),
            version_comment: vc.map(|s| s.to_string()),
            file: "ci.yml".into(),
            line: 4,
            ref_start: 0,
            ref_end: current_ref.len(),
        }
    }

    fn tag(name: &str, sha: &str, major: u32, minor: u32, patch: u32) -> Tag {
        Tag {
            name: name.to_string(),
            sha: sha.to_string(),
            version: Version {
                major,
                minor,
                patch,
            },
        }
    }

    #[test]
    fn classify_version_ref() {
        assert!(matches!(
            classify(&action("a", "b", "v1.2.3", None), false),
            Some(CurrentRefKind::Version)
        ));
    }

    #[test]
    fn classify_sha_ref() {
        assert!(matches!(
            classify(&action("a", "b", "abcdef0123456789", None), false),
            Some(CurrentRefKind::Sha)
        ));
    }

    #[test]
    fn classify_branch_ref() {
        assert!(matches!(
            classify(&action("a", "b", "main", None), false),
            Some(CurrentRefKind::Branch)
        ));
    }

    #[test]
    fn classify_skip_branches() {
        assert!(classify(&action("a", "b", "main", None), true).is_none());
    }

    #[test]
    fn best_target_major_mode() {
        let tags = vec![tag("v1.2.3", "a", 1, 2, 3), tag("v2.0.0", "b", 2, 0, 0)];
        let cur = Version {
            major: 1,
            minor: 2,
            patch: 0,
        };
        assert_eq!(
            "v2.0.0",
            best_target(&tags, Some(cur), UpdateMode::Major)
                .unwrap()
                .name
        );
    }

    #[test]
    fn best_target_minor_mode() {
        let tags = vec![
            tag("v1.2.3", "a", 1, 2, 3),
            tag("v1.3.0", "b", 1, 3, 0),
            tag("v2.0.0", "c", 2, 0, 0),
        ];
        let cur = Version {
            major: 1,
            minor: 2,
            patch: 0,
        };
        assert_eq!(
            "v1.3.0",
            best_target(&tags, Some(cur), UpdateMode::Minor)
                .unwrap()
                .name
        );
    }

    #[test]
    fn best_target_patch_mode() {
        let tags = vec![tag("v1.2.3", "a", 1, 2, 3), tag("v1.2.5", "b", 1, 2, 5)];
        let cur = Version {
            major: 1,
            minor: 2,
            patch: 0,
        };
        assert_eq!(
            "v1.2.5",
            best_target(&tags, Some(cur), UpdateMode::Patch)
                .unwrap()
                .name
        );
    }

    #[test]
    fn best_target_no_current_version() {
        let tags = vec![tag("v2.0.0", "a", 2, 0, 0), tag("v1.0.0", "b", 1, 0, 0)];
        assert_eq!(
            "v2.0.0",
            best_target(&tags, None, UpdateMode::Major).unwrap().name
        );
    }

    #[test]
    fn best_target_empty_tags() {
        assert!(
            best_target(
                &[],
                Some(Version {
                    major: 1,
                    minor: 0,
                    patch: 0
                }),
                UpdateMode::Major
            )
            .is_none()
        );
    }

    #[test]
    fn current_ref_matches_style_tag_exact() {
        let t = tag("v4.2.0", "sha", 4, 2, 0);
        assert!(current_ref_matches_style("v4.2.0", &t, PinStyle::Tag));
    }

    #[test]
    fn current_ref_matches_style_tag_mismatch() {
        let t = tag("v4.2.0", "sha", 4, 2, 0);
        assert!(!current_ref_matches_style("v4.1.0", &t, PinStyle::Tag));
    }

    #[test]
    fn current_ref_matches_style_sha_exact() {
        let t = tag("v4.2.0", "abc123", 4, 2, 0);
        assert!(current_ref_matches_style("abc123", &t, PinStyle::Sha));
    }

    #[test]
    fn current_ref_matches_style_sha_prefix() {
        let t = tag("v4.2.0", "abc123456789", 4, 2, 0);
        assert!(current_ref_matches_style("abc", &t, PinStyle::Sha));
    }
}
