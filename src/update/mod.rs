pub mod output;

use crate::{
    config::{Config, PinStyle, UpdateLevel},
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
    let min_age = config.min_release_age.as_deref().and_then(parse_duration);

    for action_ref in references {
        if config.skip_branches && is_branch_like_ref(&action_ref.ref_name) {
            continue;
        }

        let tags = github_tags.tags_for_repo(&action_ref.owner, &action_ref.name)?;
        let target_tag = match config.update_level {
            Some(level) => newest_tag_for_level(&tags, &action_ref.ref_name, level),
            None => newest_version_tag(&tags),
        };
        let Some(target_tag) = target_tag else {
            continue;
        };

        let target_tag = if let Some(duration) = min_age {
            filter_by_release_age(github_tags, action_ref, &tags, target_tag, duration)?
        } else {
            target_tag
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

fn filter_by_release_age<'tags>(
    github_tags: &GitHubTags,
    action_ref: &ActionRef,
    tags: &'tags [GitHubTag],
    preferred: &'tags GitHubTag,
    min_age: std::time::Duration,
) -> Result<&'tags GitHubTag, String> {
    let cutoff = std::time::SystemTime::now() - min_age;

    if let Some(date) =
        github_tags.release_date_for_tag(&action_ref.owner, &action_ref.name, &preferred.name, &preferred.sha)?
        && parse_datetime(&date)? <= cutoff {
            return Ok(preferred);
        }

    let mut allowed: Vec<&GitHubTag> = Vec::new();
    for tag in tags {
        if let Some(date) =
            github_tags.release_date_for_tag(&action_ref.owner, &action_ref.name, &tag.name, &tag.sha)?
            && parse_datetime(&date)? <= cutoff {
                allowed.push(tag);
            }
    }

    Ok(allowed
        .into_iter()
        .max_by_key(|tag| version_key(&tag.name))
        .unwrap_or(preferred))
}

fn parse_duration(value: &str) -> Option<std::time::Duration> {
    let value = value.trim();
    let (number, unit) = value.split_at(value.len().checked_sub(1)?);
    let number = number.parse::<u64>().ok()?;

    match unit {
        "m" => Some(std::time::Duration::from_secs(number * 60)),
        "h" => Some(std::time::Duration::from_secs(number * 60 * 60)),
        "d" => Some(std::time::Duration::from_secs(number * 60 * 60 * 24)),
        _ => None,
    }
}

fn parse_datetime(value: &str) -> Result<std::time::SystemTime, String> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(value, &Rfc3339)
        .map(|datetime| std::time::UNIX_EPOCH + std::time::Duration::from_secs(datetime.unix_timestamp() as u64))
        .map_err(|error| format!("failed to parse release date {value:?}: {error}"))
}

fn newest_version_tag(tags: &[GitHubTag]) -> Option<&GitHubTag> {
    tags.iter().max_by_key(|tag| version_key(&tag.name))
}

fn newest_tag_for_level<'tags>(
    tags: &'tags [GitHubTag],
    current_ref: &str,
    level: UpdateLevel,
) -> Option<&'tags GitHubTag> {
    let current_key = version_key(current_ref);
    tags.iter()
        .filter(|tag| match level {
            UpdateLevel::Major => true,
            UpdateLevel::Minor => {
                let key = version_key(&tag.name);
                key.first().copied().unwrap_or(0)
                    == current_key.first().copied().unwrap_or(0)
            }
            UpdateLevel::Patch => {
                let key = version_key(&tag.name);
                key.first().copied().unwrap_or(0)
                    == current_key.first().copied().unwrap_or(0)
                    && key.get(1).copied().unwrap_or(0)
                        == current_key.get(1).copied().unwrap_or(0)
            }
        })
        .max_by_key(|tag| version_key(&tag.name))
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

fn is_branch_like_ref(ref_name: &str) -> bool {
    !(is_full_sha(ref_name)
        || crate::audit::is_version_tag(ref_name)
        || (ref_name.len() < 40
            && ref_name
                .chars()
                .all(|character| character.is_ascii_hexdigit())))
}


