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

        let current_version = parse_version(&action.current_ref);
        let Some(target) = best_target(repo_tags, current_version, config.mode) else {
            continue;
        };

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
    if is_likely_sha(&action.current_ref) {
        return Some(CurrentRefKind::Sha);
    }
    if skip_branches {
        return None;
    }
    Some(CurrentRefKind::Branch)
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
    use crate::model::Version;

    use super::*;

    fn make_action(owner: &str, name: &str, current_ref: &str, vc: Option<&str>) -> Action {
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

    fn t(name: &str, sha: &str, major: u32, minor: u32, patch: u32) -> Tag {
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
    fn best_tag_respects_mode() {
        let tags = vec![
            t("v1.2.3", "a", 1, 2, 3),
            t("v1.3.0", "b", 1, 3, 0),
            t("v2.0.0", "c", 2, 0, 0),
        ];
        let cur = Version {
            major: 1,
            minor: 2,
            patch: 0,
        };
        assert_eq!(
            "v1.2.3",
            best_target(&tags, Some(cur), UpdateMode::Patch)
                .unwrap()
                .name
        );
        assert_eq!(
            "v1.3.0",
            best_target(&tags, Some(cur), UpdateMode::Minor)
                .unwrap()
                .name
        );
        assert_eq!(
            "v2.0.0",
            best_target(&tags, Some(cur), UpdateMode::Major)
                .unwrap()
                .name
        );
    }

    #[test]
    fn detects_sha_mismatch() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action(
            "actions",
            "checkout",
            "badcafe",
            Some("v4.2.0"),
        )];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert_eq!(1, actions.len());
        assert!(actions[0].sha_mismatch);
        assert!(actions[0].needs_update);
    }

    #[test]
    fn pin_style_tag_uses_tag_name() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action("actions", "checkout", "v4.1.0", None)];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Tag,
            },
        );

        assert_eq!(1, actions.len());
        assert_eq!("v4.2.0", actions[0].new_ref);
    }

    #[test]
    fn includes_branch_references_by_default() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action("actions", "checkout", "main", None)];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert!(actions[0].is_branch);
        assert!(actions[0].needs_update);
    }

    #[test]
    fn skip_branches_excludes_branch_refs() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action("actions", "checkout", "main", None)];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: true,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert!(!actions[0].needs_update);
    }

    #[test]
    fn excludes_matching_actions() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action("actions", "checkout", "v4.1.0", None)];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec!["actions/checkout".into()],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert!(!actions[0].needs_update);
    }

    #[test]
    fn no_tags_for_repository_skips_action() {
        let tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        let mut actions = vec![make_action("actions", "checkout", "v4", None)];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert!(!actions[0].needs_update);
    }

    #[test]
    fn already_up_to_date_skips_update() {
        let tags = HashMap::from([(
            ("actions".into(), "checkout".into()),
            vec![t("v4.2.0", "abcdef0123456789", 4, 2, 0)],
        )]);
        let mut actions = vec![make_action(
            "actions",
            "checkout",
            "abcdef0123456789",
            Some("v4.2.0"),
        )];
        resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );

        assert!(!actions[0].needs_update);
    }

    mod integration {
        use std::collections::HashMap;
        use std::fs;

        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use crate::github::GitHubClient;
        use crate::model::{PinStyle, ResolveConfig, UpdateMode};
        use crate::{resolve, scan};

        #[tokio::test(flavor = "multi_thread")]
        async fn scan_and_resolve_end_to_end() {
            let tmp = std::env::temp_dir().join(format!("actioneer-e2e-{}", std::process::id()));
            fs::create_dir_all(&tmp).unwrap();

            fs::write(
                tmp.join("ci.yml"),
                concat!(
                    "jobs:\n",
                    "  build:\n",
                    "    steps:\n",
                    "      - uses: actions/checkout@v4 # v4.1.0\n",
                ),
            )
            .unwrap();

            let server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/repos/actions/checkout/tags"))
                .and(query_param("per_page", "100"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    r#"[{"name":"v4.2.0","commit":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}},{"name":"v4.1.0","commit":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]"#,
                    "application/json",
                ))
                .mount(&server)
                .await;

            let result = tokio::task::block_in_place(|| {
                let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
                assert_eq!(1, actions.len());
                assert_eq!("v4", actions[0].current_ref);
                assert_eq!(Some("v4.1.0".to_string()), actions[0].version_comment);

                let gh = GitHubClient::new_for_test(false, server.uri(), None);

                let mut tags: HashMap<(String, String), Vec<crate::model::Tag>> = HashMap::new();
                let fetched = gh.fetch_tags("actions", "checkout").unwrap();
                assert_eq!(2, fetched.len());
                assert_eq!("v4.2.0", fetched[0].name);
                tags.insert(("actions".into(), "checkout".into()), fetched);

                resolve::resolve(
                    &mut actions,
                    &tags,
                    &ResolveConfig {
                        excludes: vec![],
                        skip_branches: false,
                        mode: UpdateMode::Major,
                        style: PinStyle::Sha,
                    },
                );

                actions
            });

            assert!(result[0].needs_update, "action should need update");
            assert_eq!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", result[0].new_ref);
            assert_eq!("v4.2.0", result[0].new_version);

            fs::remove_dir_all(tmp).unwrap();
        }
    }
}
