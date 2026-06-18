pub mod fix;
pub mod output;

use crate::{
    config::{Config, PinStyle},
    discovery::ActionRef,
    github::GitHubTags,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    MutableRef,
    ShaCommentMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: String,
    pub kind: FindingKind,
    pub action: ActionRef,
    pub message: String,
    pub recommendation: String,
    pub fixable: bool,
    pub expected_sha: Option<String>,
}

impl Finding {
    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            FindingKind::MutableRef => "mutable_ref",
            FindingKind::ShaCommentMismatch => "sha_comment_mismatch",
        }
    }

    pub fn severity(&self) -> &'static str {
        "error"
    }
}

pub fn audit_references(
    references: &[ActionRef],
    config: &Config,
    github_tags: &GitHubTags,
) -> Result<Vec<Finding>, String> {
    let mut findings = Vec::new();

    for (index, action_ref) in references.iter().enumerate() {
        let pin = config.effective_pin(action_ref);
        if violates_policy(action_ref, pin) {
            findings.push(mutable_ref_finding(index + 1, action_ref));
            continue;
        }

        if pin == PinStyle::Sha && is_full_sha(&action_ref.ref_name) {
            if let Some(expected_sha) = verify_sha_comment(action_ref, github_tags)? {
                findings.push(sha_comment_mismatch_finding(index + 1, action_ref, expected_sha));
            }
        }
    }

    Ok(findings)
}

pub fn fixable_count(findings: &[Finding]) -> usize {
    findings.iter().filter(|finding| finding.fixable).count()
}

fn mutable_ref_finding(id: usize, action_ref: &ActionRef) -> Finding {
    Finding {
        id: format!("finding-{id}"),
        kind: FindingKind::MutableRef,
        action: action_ref.clone(),
        message: "Action is pinned to a mutable tag".to_string(),
        recommendation: "Pin to a full SHA".to_string(),
        fixable: true,
        expected_sha: None,
    }
}

fn sha_comment_mismatch_finding(
    id: usize,
    action_ref: &ActionRef,
    expected_sha: String,
) -> Finding {
    Finding {
        id: format!("finding-{id}"),
        kind: FindingKind::ShaCommentMismatch,
        action: action_ref.clone(),
        message: "SHA comment does not match the tagged SHA".to_string(),
        recommendation: "Update the pinned SHA to match the version comment".to_string(),
        fixable: true,
        expected_sha: Some(expected_sha),
    }
}

fn verify_sha_comment(
    action_ref: &ActionRef,
    github_tags: &GitHubTags,
) -> Result<Option<String>, String> {
    let Some(comment) = &action_ref.version_comment else {
        return Ok(None);
    };

    let tags = github_tags.tags_for_repo(&action_ref.owner, &action_ref.name)?;

    let Some(tag) = tags.iter().find(|tag| tag.name == *comment) else {
        return Ok(None);
    };

    if tag.sha == action_ref.ref_name {
        Ok(None)
    } else {
        Ok(Some(tag.sha.clone()))
    }
}

fn violates_policy(action_ref: &ActionRef, pin: PinStyle) -> bool {
    match pin {
        PinStyle::Sha => !is_full_sha(&action_ref.ref_name),
        PinStyle::Tag => {
            !is_full_sha(&action_ref.ref_name) && !is_version_tag(&action_ref.ref_name)
        }
    }
}

fn is_full_sha(ref_name: &str) -> bool {
    ref_name.len() == 40
        && ref_name
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

pub fn is_version_tag(ref_name: &str) -> bool {
    let Some(version) = ref_name.strip_prefix('v') else {
        return false;
    };

    !version.is_empty()
        && version.chars().any(|character| character.is_ascii_digit())
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
}
