pub mod output;

use crate::{
    config::{Config, PinStyle},
    discovery::DiscoveredActionRef,
    github::{GitHubTag, GitHubTags},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    pub references: usize,
    pub candidates: Vec<UpdateCandidate>,
}

impl UpdatePlan {
    pub fn selected_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .count()
    }

    pub fn applied_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.applied)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub id: String,
    pub kind: UpdateKind,
    pub file: String,
    pub line: usize,
    pub action: UpdateAction,
    pub target: UpdateTarget,
    pub reason: UpdateReason,
    pub notes: Vec<UpdateNote>,
    pub selected: bool,
    pub applied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    VersionUpdate,
}

impl UpdateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VersionUpdate => "version_update",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAction {
    pub owner: String,
    pub name: String,
    pub repo: String,
    pub path: String,
    pub current_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTarget {
    pub ref_name: String,
    pub version: String,
    pub sha: String,
    pub pin: PinStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateReason {
    NewerVersionAvailable,
}

impl UpdateReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewerVersionAvailable => "newer_version_available",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateNote {
    MutableRef,
}

impl UpdateNote {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MutableRef => "mutable_ref",
        }
    }
}

pub fn plan_update_candidates(
    references: &[DiscoveredActionRef],
    config: &Config,
    github_tags: &GitHubTags,
    dry_run: bool,
) -> Result<UpdatePlan, String> {
    let mut candidates = Vec::new();

    for action_ref in references {
        let tags = github_tags.tags_for_repo(&action_ref.owner, &action_ref.name)?;
        let Some(target_tag) = newest_version_tag(&tags) else {
            continue;
        };

        let pin = config.effective_pin(action_ref);
        let target_ref = match pin {
            PinStyle::Sha => target_tag.sha.clone(),
            PinStyle::Tag => target_tag.name.clone(),
        };

        if action_ref.ref_name == target_ref {
            continue;
        }

        candidates.push(UpdateCandidate {
            id: format!("update-{}", candidates.len() + 1),
            kind: UpdateKind::VersionUpdate,
            file: action_ref.file.display().to_string(),
            line: action_ref.line,
            action: UpdateAction {
                owner: action_ref.owner.clone(),
                name: action_ref.name.clone(),
                repo: action_ref.repo.clone(),
                path: action_ref.path.clone(),
                current_ref: action_ref.ref_name.clone(),
            },
            target: UpdateTarget {
                ref_name: target_ref,
                version: target_tag.name.clone(),
                sha: target_tag.sha.clone(),
                pin,
            },
            reason: UpdateReason::NewerVersionAvailable,
            notes: update_notes(action_ref),
            selected: dry_run,
            applied: false,
        });
    }

    Ok(UpdatePlan {
        references: references.len(),
        candidates,
    })
}

fn update_notes(action_ref: &DiscoveredActionRef) -> Vec<UpdateNote> {
    if is_full_sha(&action_ref.ref_name) {
        Vec::new()
    } else {
        vec![UpdateNote::MutableRef]
    }
}

fn newest_version_tag(tags: &[GitHubTag]) -> Option<&GitHubTag> {
    tags.iter().max_by_key(|tag| version_key(&tag.name))
}

fn version_key(tag: &str) -> Vec<u64> {
    tag.strip_prefix('v')
        .unwrap_or(tag)
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_full_sha(ref_name: &str) -> bool {
    ref_name.len() == 40 && ref_name.chars().all(|character| character.is_ascii_hexdigit())
}
