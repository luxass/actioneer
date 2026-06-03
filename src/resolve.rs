use std::collections::HashMap;

use crate::model::{
    Action, PinStyle, ResolveConfig, Tag, UpdateMode, Version, is_likely_sha, parse_version,
    sha_matches,
};

#[derive(Clone, Copy, Debug)]
enum CurrentRefKind {
    Version,
    Sha,
    Branch,
}

pub fn resolve(
    actions: &mut [Action],
    tags: &HashMap<(String, String), Vec<Tag>>,
    config: &ResolveConfig,
) {
    for action in actions.iter_mut() {
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

        action.expected_sha = if let Some(tag) = comment_tag {
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

        action.sha_mismatch = matches!(ctx, CurrentRefKind::Sha)
            && comment_tag
                .map(|t| !sha_matches(&action.current_ref, &t.sha))
                .unwrap_or(false);

        let current_version = parse_version(&action.current_ref)
            .or_else(|| action.version_comment.as_deref().and_then(parse_version));
        let Some(target) = best_target(repo_tags, current_version, config.mode) else {
            continue;
        };

        if let Some(cur) = current_version
            && target.version < cur
        {
            continue;
        }

        if !action.sha_mismatch
            && current_ref_matches_style(&action.current_ref, target, config.style)
        {
            continue;
        }

        action.is_branch = matches!(ctx, CurrentRefKind::Branch);
        action.is_major = current_version
            .map(|v| target.version.major > v.major)
            .unwrap_or(false);
        action.new_ref = match config.style {
            PinStyle::Sha => target.sha.clone(),
            PinStyle::Tag => target.name.clone(),
        };
        action.new_version = target.name.clone();
        action.needs_update = true;
    }
}

fn classify(action: &Action, skip_branches: bool) -> Option<CurrentRefKind> {
    if parse_version(&action.current_ref).is_some() {
        return Some(CurrentRefKind::Version);
    }
    if is_likely_sha(&action.current_ref)
        || (action.version_comment.is_some() && is_sha_like_ref(&action.current_ref))
    {
        return Some(CurrentRefKind::Sha);
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
    use std::collections::HashMap;

    use crate::model::Version;

    use super::*;

    fn action(owner: &str, name: &str, current_ref: &str, vc: Option<&str>) -> Action {
        Action {
            owner: owner.to_string(),
            name: name.to_string(),
            path: String::new(),
            current_ref: current_ref.to_string(),
            version_comment: vc.map(|s| s.to_string()),
            file: "ci.yml".into(),
            line: 4,
            ref_start: 0,
            ref_end: current_ref.len(),
            new_ref: String::new(),
            new_version: String::new(),
            expected_sha: String::new(),
            sha_mismatch: false,
            is_branch: false,
            is_major: false,
            needs_update: false,
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

    fn config() -> ResolveConfig {
        ResolveConfig {
            excludes: vec![],
            skip_branches: false,
            mode: UpdateMode::Major,
            style: PinStyle::Sha,
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

    #[test]
    fn resolve_detects_version_upgrade() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![tag("v4.2.0", "sha42", 4, 2, 0)],
        )]);
        let mut actions = vec![action("actions", "checkout", "v4.1.0", None)];
        resolve(&mut actions, &tags, &config());
        assert!(actions[0].needs_update);
        assert_eq!("sha42", actions[0].new_ref);
    }

    #[test]
    fn resolve_detects_branch() {
        let tags = HashMap::from([(
            ("a".into(), "b".into()),
            vec![tag("v1.0.0", "sha1", 1, 0, 0)],
        )]);
        let mut actions = vec![action("a", "b", "main", None)];
        resolve(&mut actions, &tags, &config());
        assert!(actions[0].is_branch);
        assert!(actions[0].needs_update);
    }

    #[test]
    fn resolve_skip_branches_ignores() {
        let tags = HashMap::from([(
            ("a".into(), "b".into()),
            vec![tag("v1.0.0", "sha1", 1, 0, 0)],
        )]);
        let mut actions = vec![action("a", "b", "main", None)];
        let cfg = ResolveConfig {
            skip_branches: true,
            ..config()
        };
        resolve(&mut actions, &tags, &cfg);
        assert!(!actions[0].needs_update);
    }

    #[test]
    fn resolve_sha_mismatch() {
        let tags = HashMap::from([(
            ("a".into(), "b".into()),
            vec![tag("v4.2.0", "goodsha", 4, 2, 0)],
        )]);
        let mut actions = vec![action("a", "b", "badcafe0", Some("v4.2.0"))];
        resolve(&mut actions, &tags, &config());
        assert!(actions[0].sha_mismatch);
        assert!(actions[0].needs_update);
    }

    #[test]
    fn resolve_long_sha_typo_as_mismatch_not_branch() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![
                tag(
                    "v6.0.2",
                    "de0fac2e4500dabe0009e67214ff5f5447ce83dd",
                    6,
                    0,
                    2,
                ),
                tag(
                    "v6.0.3",
                    "df4cb1c069e1874edd31b4311f1884172cec0e10",
                    6,
                    0,
                    3,
                ),
            ],
        )]);
        let mut actions = vec![action(
            "actions",
            "checkout",
            "de0fac2ea4500dabe0009e67214ff5f5447ce83dd",
            Some("v6.0.2"),
        )];
        resolve(&mut actions, &tags, &config());
        assert!(actions[0].sha_mismatch);
        assert!(!actions[0].is_branch);
        assert!(actions[0].needs_update);
    }

    #[test]
    fn resolve_excluded_action() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![tag("v4.2.0", "sha42", 4, 2, 0)],
        )]);
        let mut actions = vec![action("actions", "checkout", "v4.1.0", None)];
        let cfg = ResolveConfig {
            excludes: vec!["actions/checkout".into()],
            ..config()
        };
        resolve(&mut actions, &tags, &cfg);
        assert!(!actions[0].needs_update);
    }

    #[test]
    fn resolve_detects_major_bump() {
        let tags = HashMap::from([(
            ("a".into(), "b".into()),
            vec![tag("v3.0.0", "s1", 3, 0, 0), tag("v4.0.0", "s2", 4, 0, 0)],
        )]);
        let mut actions = vec![action("a", "b", "v3.0.0", None)];
        resolve(&mut actions, &tags, &config());
        assert!(actions[0].is_major);
    }

    #[test]
    fn resolve_skips_downgrade() {
        let tags = HashMap::from([(
            ("a".into(), "b".into()),
            vec![tag("v4.2.0", "sha42", 4, 2, 0)],
        )]);
        let mut actions = vec![action("a", "b", "v5.0.0", None)];
        resolve(&mut actions, &tags, &config());
        assert!(!actions[0].needs_update);
    }
}
