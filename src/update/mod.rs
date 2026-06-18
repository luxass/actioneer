pub mod output;

use crate::{
    config::{Config, PinStyle},
    discovery::ActionRef,
    github::{GitHubTag, GitHubTags},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub action: ActionRef,
    pub target_ref: String,
    pub version: String,
    pub sha: String,
    pub pin: PinStyle,
    pub notes: Vec<&'static str>,
}

pub fn all_candidate_indexes(candidates: &[Candidate]) -> Vec<usize> {
    (0..candidates.len()).collect()
}

pub fn plan_update_candidates(
    references: &[ActionRef],
    config: &Config,
    github_tags: &GitHubTags,
) -> Result<Vec<Candidate>, String> {
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

        candidates.push(Candidate {
            id: format!("update-{}", candidates.len() + 1),
            action: action_ref.clone(),
            target_ref,
            version: target_tag.name.clone(),
            sha: target_tag.sha.clone(),
            pin,
            notes: update_notes(action_ref),
        });
    }

    Ok(candidates)
}

fn update_notes(action_ref: &ActionRef) -> Vec<&'static str> {
    if is_full_sha(&action_ref.ref_name) {
        Vec::new()
    } else {
        vec!["mutable_ref"]
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
    ref_name.len() == 40
        && ref_name
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}
