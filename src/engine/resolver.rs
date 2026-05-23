use std::collections::{hash_map::Entry, HashMap};

use thiserror::Error;

use crate::engine::git::{is_likely_sha, parse_version, sha_matches, Version};
use crate::github::{Error as GitHubError, Tag};
use crate::model::{
    PinStyle, Reference, Repository, ResolveOptions, ResolvedUpdate, UpdateMode, UpdateSource,
    UpdateTarget, ValidationState,
};

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("github request failed for {repository}")]
    GitHub {
        repository: String,
        #[source]
        source: GitHubError,
    },
}

#[derive(Clone, Copy, Debug)]
enum CurrentRefKind {
    Version,
    Sha,
    Branch,
}

#[derive(Clone, Copy, Debug)]
struct UpdateContext {
    ref_kind: CurrentRefKind,
    current_version: Option<Version>,
}

pub fn resolve_updates(
    fetch_tags: &impl Fn(&Repository) -> Result<Vec<Tag>, GitHubError>,
    references: &[Reference],
    options: &ResolveOptions,
) -> Result<(Vec<ResolvedUpdate>, usize), ResolveError> {
    let mut tag_cache: HashMap<Repository, Vec<Tag>> = HashMap::new();
    let mut updates = Vec::new();
    let mut branch_ref_count = 0;

    for reference in references {
        if is_excluded(reference, &options.excludes) {
            continue;
        }

        let Some(context) = classify_reference(reference, options.skip_branches) else {
            continue;
        };

        let repository = &reference.name.repository;
        let Some(tags) = repository_tags(fetch_tags, repository, &mut tag_cache)? else {
            continue;
        };

        let comment_tag = version_comment_tag(tags, &reference.version_hint);
        let sha_mismatch = matches!(context.ref_kind, CurrentRefKind::Sha)
            && comment_tag
                .map(|tag| !sha_matches(&reference.current_ref, &tag.sha))
                .unwrap_or(false);

        let Some(target_tag) = target_tag(tags, context.current_version, options.mode) else {
            continue;
        };

        if !sha_mismatch && current_ref_matches_target(reference, target_tag) {
            continue;
        }

        let is_branch = matches!(context.ref_kind, CurrentRefKind::Branch);
        if is_branch {
            branch_ref_count += 1;
        }

        updates.push(ResolvedUpdate::new(
            reference.name.display(),
            reference.scope.clone(),
            reference.current_ref.clone(),
            ValidationState::new(
                expected_current_sha(reference, context.ref_kind, comment_tag, tags),
                reference.version_hint.clone(),
                sha_mismatch,
            ),
            UpdateTarget::new(
                match options.style {
                    PinStyle::Sha => target_tag.sha.clone(),
                    PinStyle::Tag => target_tag.name.clone(),
                },
                target_tag.name.clone(),
                is_major_update(context.current_version, target_tag.version),
            ),
            UpdateSource::new(
                reference.source.file.clone(),
                reference.source.line,
                reference.source.ref_span.start,
                reference.source.ref_span.end,
            ),
            is_branch,
        ));
    }

    Ok((updates, branch_ref_count))
}
fn is_excluded(reference: &Reference, excludes: &[String]) -> bool {
    let action = reference.name.display();
    excludes.iter().any(|exclude| action.contains(exclude))
}

fn classify_reference(reference: &Reference, skip_branches: bool) -> Option<UpdateContext> {
    let comment_version = parse_version(&reference.version_hint);

    if let Some(version) = parse_version(&reference.current_ref) {
        return Some(UpdateContext {
            ref_kind: CurrentRefKind::Version,
            current_version: Some(version),
        });
    }

    if is_likely_sha(&reference.current_ref) {
        return Some(UpdateContext {
            ref_kind: CurrentRefKind::Sha,
            current_version: comment_version,
        });
    }

    if skip_branches {
        return None;
    }

    Some(UpdateContext {
        ref_kind: CurrentRefKind::Branch,
        current_version: None,
    })
}

fn repository_tags<'a>(
    fetch_tags: &impl Fn(&Repository) -> Result<Vec<Tag>, GitHubError>,
    repository: &Repository,
    cache: &'a mut HashMap<Repository, Vec<Tag>>,
) -> Result<Option<&'a [Tag]>, ResolveError> {
    match cache.entry(repository.clone()) {
        Entry::Occupied(entry) => Ok(Some(entry.into_mut().as_slice())),
        Entry::Vacant(entry) => {
            let tags = fetch_tags(repository).map_err(|source| ResolveError::GitHub {
                repository: repository.display(),
                source,
            })?;
            if tags.is_empty() {
                return Ok(None);
            }
            Ok(Some(entry.insert(tags).as_slice()))
        }
    }
}

fn version_comment_tag<'a>(tags: &'a [Tag], version_hint: &str) -> Option<&'a Tag> {
    (!version_hint.is_empty())
        .then(|| tags.iter().find(|tag| tag.name == version_hint))
        .flatten()
}

fn target_tag(tags: &[Tag], current: Option<Version>, mode: UpdateMode) -> Option<&Tag> {
    tags.iter()
        .filter(|tag| match current {
            Some(current_version) => match mode {
                UpdateMode::Minor => tag.version.major == current_version.major,
                UpdateMode::Patch => {
                    tag.version.major == current_version.major
                        && tag.version.minor == current_version.minor
                }
                UpdateMode::Major => true,
            },
            None => true,
        })
        .max_by_key(|tag| tag.version)
}

fn current_ref_matches_target(reference: &Reference, target: &Tag) -> bool {
    reference.current_ref == target.name || reference.current_ref == target.sha
}

fn expected_current_sha(
    reference: &Reference,
    ref_kind: CurrentRefKind,
    comment_tag: Option<&Tag>,
    tags: &[Tag],
) -> String {
    if let Some(tag) = comment_tag {
        return tag.sha.clone();
    }

    if let Some(tag) = tags.iter().find(|tag| {
        tag.name == reference.current_ref
            || tag.sha == reference.current_ref
            || tag.sha.starts_with(&reference.current_ref)
    }) {
        return tag.sha.clone();
    }

    if matches!(ref_kind, CurrentRefKind::Sha) {
        return reference.current_ref.clone();
    }

    String::new()
}

fn is_major_update(current: Option<Version>, target: Version) -> bool {
    current
        .map(|version| target.major > version.major)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;

    use crate::engine::git::Version;
    use crate::github::{Error as GitHubError, Tag};
    use crate::model::{
        ActionName, ByteSpan, PinStyle, Reference, ReferenceKind, Repository, ResolveOptions,
        SourceLocation, UpdateMode,
    };

    use super::*;

    #[test]
    fn target_tag_respects_mode() {
        let tags = vec![
            Tag {
                name: "v1.2.3".into(),
                sha: "a".into(),
                version: Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                },
            },
            Tag {
                name: "v1.3.0".into(),
                sha: "b".into(),
                version: Version {
                    major: 1,
                    minor: 3,
                    patch: 0,
                },
            },
            Tag {
                name: "v2.0.0".into(),
                sha: "c".into(),
                version: Version {
                    major: 2,
                    minor: 0,
                    patch: 0,
                },
            },
        ];

        assert_eq!(
            "v1.2.3",
            target_tag(
                &tags,
                Some(Version {
                    major: 1,
                    minor: 2,
                    patch: 0,
                }),
                UpdateMode::Patch,
            )
            .unwrap()
            .name
        );

        assert_eq!(
            "v1.3.0",
            target_tag(
                &tags,
                Some(Version {
                    major: 1,
                    minor: 2,
                    patch: 0,
                }),
                UpdateMode::Minor,
            )
            .unwrap()
            .name
        );

        assert_eq!(
            "v2.0.0",
            target_tag(
                &tags,
                Some(Version {
                    major: 1,
                    minor: 2,
                    patch: 0,
                }),
                UpdateMode::Major,
            )
            .unwrap()
            .name
        );
    }

    #[test]
    fn detects_sha_mismatch() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let tags = HashMap::from([(
            repository.clone(),
            vec![Tag {
                name: "v4.2.0".into(),
                sha: "abcdef0123456789".into(),
                version: Version {
                    major: 4,
                    minor: 2,
                    patch: 0,
                },
            }],
        )]);
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "badcafe".into(),
            version_hint: "v4.2.0".into(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 7 },
            },
        };

        let (updates, _) = resolve_updates(
            &|repository| Ok(tags.get(repository).cloned().unwrap_or_default()),
            &[reference],
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();

        assert_eq!(1, updates.len());
        assert!(updates[0].has_sha_mismatch());
    }

    #[test]
    fn pin_style_tag_uses_tag_name_instead_of_sha() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let tags = HashMap::from([(
            repository.clone(),
            vec![Tag {
                name: "v4.2.0".into(),
                sha: "abcdef0123456789".into(),
                version: Version {
                    major: 4,
                    minor: 2,
                    patch: 0,
                },
            }],
        )]);
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "v4.1.0".into(),
            version_hint: String::new(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 6 },
            },
        };

        let (updates, _) = resolve_updates(
            &|repository| Ok(tags.get(repository).cloned().unwrap_or_default()),
            &[reference],
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Tag,
            },
        )
        .unwrap();

        assert_eq!(1, updates.len());
        assert_eq!("v4.2.0", updates[0].next_ref());
        assert_eq!("v4.2.0", updates[0].display_target());
    }

    #[test]
    fn reuses_tag_cache_for_same_repository() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let calls = Cell::new(0);
        let tags = HashMap::from([(
            repository.clone(),
            vec![Tag {
                name: "v4.2.0".into(),
                sha: "abcdef0123456789".into(),
                version: Version {
                    major: 4,
                    minor: 2,
                    patch: 0,
                },
            }],
        )]);
        let references = vec![
            Reference {
                kind: ReferenceKind::WorkflowStep,
                name: ActionName {
                    repository: repository.clone(),
                    path: String::new(),
                },
                current_ref: "v4.1.0".into(),
                version_hint: String::new(),
                scope: "build".into(),
                source: SourceLocation {
                    file: "ci.yml".into(),
                    line: 4,
                    ref_span: ByteSpan { start: 0, end: 6 },
                },
            },
            Reference {
                kind: ReferenceKind::WorkflowStep,
                name: ActionName {
                    repository,
                    path: String::new(),
                },
                current_ref: "v4.0.0".into(),
                version_hint: String::new(),
                scope: "test".into(),
                source: SourceLocation {
                    file: "ci.yml".into(),
                    line: 8,
                    ref_span: ByteSpan { start: 0, end: 6 },
                },
            },
        ];

        let _ = resolve_updates(
            &|repository| {
                calls.set(calls.get() + 1);
                Ok(tags.get(repository).cloned().unwrap_or_default())
            },
            &references,
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();

        assert_eq!(1, calls.get());
    }

    #[test]
    fn includes_branch_references_by_default() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let tags = HashMap::from([(
            repository.clone(),
            vec![Tag {
                name: "v4.2.0".into(),
                sha: "abcdef0123456789".into(),
                version: Version {
                    major: 4,
                    minor: 2,
                    patch: 0,
                },
            }],
        )]);
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "main".into(),
            version_hint: String::new(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 4 },
            },
        };

        let (updates_default, branch_count_default) = resolve_updates(
            &|repository| Ok(tags.get(repository).cloned().unwrap_or_default()),
            std::slice::from_ref(&reference),
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();
        let (updates_skipped, branch_count_skipped) = resolve_updates(
            &|repository| Ok(tags.get(repository).cloned().unwrap_or_default()),
            &[reference],
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: true,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();

        assert_eq!(1, updates_default.len());
        assert!(updates_default[0].is_branch_ref());
        assert_eq!(1, branch_count_default);
        assert!(updates_skipped.is_empty());
        assert_eq!(0, branch_count_skipped);
    }

    #[test]
    fn excludes_matching_actions() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let tags = HashMap::from([(
            repository.clone(),
            vec![Tag {
                name: "v4.2.0".into(),
                sha: "abcdef0123456789".into(),
                version: Version {
                    major: 4,
                    minor: 2,
                    patch: 0,
                },
            }],
        )]);
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "v4.1.0".into(),
            version_hint: String::new(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 6 },
            },
        };

        let (updates, _) = resolve_updates(
            &|repository| Ok(tags.get(repository).cloned().unwrap_or_default()),
            &[reference],
            &ResolveOptions {
                excludes: vec![String::from("setup"), String::from("checkout")],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();

        assert!(updates.is_empty());
    }

    #[test]
    fn skips_repository_without_semver_tags() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "v4.1.0".into(),
            version_hint: String::new(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 6 },
            },
        };

        let (updates, _) = resolve_updates(
            &|_| Ok(Vec::new()),
            &[reference],
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap();

        assert!(updates.is_empty());
    }

    #[test]
    fn wraps_github_error_with_repository_context() {
        let repository = Repository {
            owner: "actions".into(),
            name: "checkout".into(),
        };
        let reference = Reference {
            kind: ReferenceKind::WorkflowStep,
            name: ActionName {
                repository,
                path: String::new(),
            },
            current_ref: "v4.1.0".into(),
            version_hint: String::new(),
            scope: "build".into(),
            source: SourceLocation {
                file: "ci.yml".into(),
                line: 4,
                ref_span: ByteSpan { start: 0, end: 6 },
            },
        };

        let err = resolve_updates(
            &|_| Err(GitHubError::HttpStatus(403)),
            &[reference],
            &ResolveOptions {
                excludes: Vec::new(),
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        )
        .unwrap_err();

        match err {
            ResolveError::GitHub { repository, .. } => assert_eq!("actions/checkout", repository),
        }
    }
}
