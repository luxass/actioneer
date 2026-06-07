use std::collections::HashMap;

use actioneer::actions::{
    ActionReference, PinStyle, ResolveConfig, Tag, UpdateMode, Version, is_likely_sha,
    parse_version, resolve, sha_matches,
};

fn action(owner: &str, name: &str, current_ref: &str, vc: Option<&str>) -> ActionReference {
    ActionReference::from_discovery(
        owner.to_string(),
        name.to_string(),
        String::new(),
        current_ref.to_string(),
        vc.map(|s| s.to_string()),
        "ci.yml".into(),
        4,
        0,
        current_ref.len(),
    )
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
fn action_name_no_path() {
    let a = ActionReference::from_discovery(
        "own".into(),
        "repo".into(),
        String::new(),
        "v1".into(),
        None,
        "f".into(),
        1,
        0,
        2,
    );
    assert_eq!("own/repo", a.action_name());
}

#[test]
fn action_name_with_path() {
    let a = ActionReference::from_discovery(
        "own".into(),
        "repo".into(),
        "/.github/workflows/ci.yml".into(),
        "v1".into(),
        None,
        "f".into(),
        1,
        0,
        2,
    );
    assert_eq!("own/repo/.github/workflows/ci.yml", a.action_name());
}

#[test]
fn parse_versions() {
    assert_eq!(
        Version {
            major: 1,
            minor: 2,
            patch: 3
        },
        parse_version("v1.2.3").unwrap()
    );
    assert_eq!(
        Version {
            major: 4,
            minor: 5,
            patch: 6
        },
        parse_version("V4.5.6").unwrap()
    );
    assert_eq!(
        Version {
            major: 7,
            minor: 8,
            patch: 9
        },
        parse_version("7.8.9").unwrap()
    );
    assert_eq!(
        Version {
            major: 1,
            minor: 0,
            patch: 0
        },
        parse_version("v1").unwrap()
    );
    assert_eq!(
        Version {
            major: 1,
            minor: 2,
            patch: 0
        },
        parse_version("v1.2").unwrap()
    );
    assert_eq!(
        Version {
            major: 1,
            minor: 2,
            patch: 3
        },
        parse_version("v1.2.3-beta").unwrap()
    );
    assert_eq!(
        Version {
            major: 0,
            minor: 1,
            patch: 0
        },
        parse_version("v0.1.0").unwrap()
    );
}

#[test]
fn parse_version_rejects_invalid_values() {
    assert!(parse_version("").is_none());
    assert!(parse_version("v").is_none());
    assert!(parse_version("not-a-version").is_none());
}

#[test]
fn detects_likely_sha_values() {
    assert!(is_likely_sha("abcdef0"));
    assert!(is_likely_sha("abcdef0123456789abcdef0123456789abcdef01"));
    assert!(!is_likely_sha("abcde"));
    assert!(!is_likely_sha("abcdef0123456789abcdef0123456789abcdef0123"));
    assert!(!is_likely_sha("abcdefg"));
    assert!(!is_likely_sha(""));
}

#[test]
fn sha_prefixes_match_full_values() {
    assert!(sha_matches("abc123", "abc123"));
    assert!(sha_matches("abc", "abc123456789"));
    assert!(!sha_matches("abc", "def456"));
}

#[test]
fn version_ordering() {
    assert!(
        Version {
            major: 2,
            minor: 0,
            patch: 0
        } > Version {
            major: 1,
            minor: 9,
            patch: 9
        }
    );
    assert!(
        Version {
            major: 1,
            minor: 3,
            patch: 0
        } > Version {
            major: 1,
            minor: 2,
            patch: 9
        }
    );
    assert!(
        Version {
            major: 1,
            minor: 2,
            patch: 5
        } > Version {
            major: 1,
            minor: 2,
            patch: 4
        }
    );
    assert_eq!(
        Version {
            major: 1,
            minor: 2,
            patch: 3
        },
        Version {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
}

#[test]
fn resolve_detects_version_upgrade() {
    let tags = HashMap::from([(
        ("actions".into(), "checkout".into()),
        vec![tag("v4.2.0", "sha42", 4, 2, 0)],
    )]);
    let actions = vec![action("actions", "checkout", "v4.1.0", None)];
    let updates = resolve(&actions, &tags, &config());
    assert_eq!(1, updates.len());
    assert_eq!("sha42", updates[0].new_ref);
}

#[test]
fn resolve_detects_branch() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v1.0.0", "sha1", 1, 0, 0)],
    )]);
    let actions = vec![action("a", "b", "main", None)];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates[0].is_branch);
}

#[test]
fn resolve_skip_branches_ignores() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v1.0.0", "sha1", 1, 0, 0)],
    )]);
    let actions = vec![action("a", "b", "main", None)];
    let cfg = ResolveConfig {
        skip_branches: true,
        ..config()
    };
    let updates = resolve(&actions, &tags, &cfg);
    assert!(updates.is_empty());
}

#[test]
fn resolve_sha_mismatch() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v4.2.0", "goodsha", 4, 2, 0)],
    )]);
    let actions = vec![action("a", "b", "badcafe0", Some("v4.2.0"))];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates[0].sha_mismatch);
}

#[test]
fn resolve_sha_mismatch_with_numeric_leading_sha() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v4.2.0", "goodsha0", 4, 2, 0)],
    )]);
    let actions = vec![action("a", "b", "1badcafe", Some("v4.2.0"))];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates[0].sha_mismatch);
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
    let actions = vec![action(
        "actions",
        "checkout",
        "de0fac2ea4500dabe0009e67214ff5f5447ce83dd",
        Some("v6.0.2"),
    )];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates[0].sha_mismatch);
    assert!(!updates[0].is_branch);
}

#[test]
fn resolve_excluded_action() {
    let tags = HashMap::from([(
        ("actions".into(), "checkout".into()),
        vec![tag("v4.2.0", "sha42", 4, 2, 0)],
    )]);
    let actions = vec![action("actions", "checkout", "v4.1.0", None)];
    let cfg = ResolveConfig {
        excludes: vec!["actions/checkout".into()],
        ..config()
    };
    let updates = resolve(&actions, &tags, &cfg);
    assert!(updates.is_empty());
}

#[test]
fn resolve_detects_major_bump() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v3.0.0", "s1", 3, 0, 0), tag("v4.0.0", "s2", 4, 0, 0)],
    )]);
    let actions = vec![action("a", "b", "v3.0.0", None)];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates[0].is_major);
}

#[test]
fn resolve_skips_downgrade() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![tag("v4.2.0", "sha42", 4, 2, 0)],
    )]);
    let actions = vec![action("a", "b", "v5.0.0", None)];
    let updates = resolve(&actions, &tags, &config());
    assert!(updates.is_empty());
}

#[test]
fn resolve_sha_pin_uses_version_comment_for_minor_mode() {
    let tags = HashMap::from([(
        ("a".into(), "b".into()),
        vec![
            tag("v4.3.0", "sha43", 4, 3, 0),
            tag("v5.0.0", "sha50", 5, 0, 0),
        ],
    )]);
    let actions = vec![action("a", "b", "deadbeef", Some("v4.2.0"))];
    let cfg = ResolveConfig {
        mode: UpdateMode::Minor,
        ..config()
    };

    let updates = resolve(&actions, &tags, &cfg);

    assert_eq!("sha43", updates[0].new_ref);
    assert_eq!("v4.3.0", updates[0].new_version);
    assert!(!updates[0].is_major);
}
