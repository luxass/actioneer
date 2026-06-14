pub mod fix;
pub mod output;

use crate::{
    config::{Config, PinStyle},
    discovery::ActionRef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: String,
    pub action: ActionRef,
    pub message: String,
    pub recommendation: String,
    pub fixable: bool,
    pub expected_sha: Option<String>,
}

impl Finding {
    pub fn kind(&self) -> &'static str {
        "mutable_ref"
    }

    pub fn severity(&self) -> &'static str {
        "error"
    }
}

pub fn audit_references(references: &[ActionRef], config: &Config) -> Vec<Finding> {
    references
        .iter()
        .filter(|action_ref| violates_policy(action_ref, config.effective_pin(action_ref)))
        .enumerate()
        .map(|(index, action_ref)| mutable_ref_finding(index + 1, action_ref))
        .collect()
}

pub fn fixable_count(findings: &[Finding]) -> usize {
    findings.iter().filter(|finding| finding.fixable).count()
}

fn mutable_ref_finding(id: usize, action_ref: &ActionRef) -> Finding {
    Finding {
        id: format!("finding-{id}"),
        action: action_ref.clone(),
        message: "Action is pinned to a mutable tag".to_string(),
        recommendation: "Pin to a full SHA".to_string(),
        fixable: true,
        expected_sha: None,
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

fn is_version_tag(ref_name: &str) -> bool {
    let Some(version) = ref_name.strip_prefix('v') else {
        return false;
    };

    !version.is_empty()
        && version.chars().any(|character| character.is_ascii_digit())
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
}
