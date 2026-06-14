use crate::discovery::DiscoveredActionRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReport {
    pub references: usize,
    pub findings: Vec<AuditFinding>,
}

impl AuditReport {
    pub fn ok(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn fixable_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.fixable)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub id: String,
    pub kind: AuditFindingKind,
    pub severity: AuditSeverity,
    pub file: String,
    pub line: usize,
    pub action: AuditAction,
    pub message: String,
    pub recommendation: String,
    pub fixable: bool,
    pub expected_sha: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditFindingKind {
    MutableRef,
}

impl AuditFindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MutableRef => "mutable_ref",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Error,
}

impl AuditSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditAction {
    pub owner: String,
    pub name: String,
    pub repo: String,
    pub path: String,
    pub ref_name: String,
}

pub fn audit_references(references: &[DiscoveredActionRef]) -> AuditReport {
    let findings = references
        .iter()
        .filter(|action_ref| !is_full_sha(&action_ref.ref_name))
        .enumerate()
        .map(|(index, action_ref)| mutable_ref_finding(index + 1, action_ref))
        .collect();

    AuditReport {
        references: references.len(),
        findings,
    }
}

fn mutable_ref_finding(id: usize, action_ref: &DiscoveredActionRef) -> AuditFinding {
    AuditFinding {
        id: format!("finding-{id}"),
        kind: AuditFindingKind::MutableRef,
        severity: AuditSeverity::Error,
        file: action_ref.file.display().to_string(),
        line: action_ref.line,
        action: AuditAction {
            owner: action_ref.owner.clone(),
            name: action_ref.name.clone(),
            repo: action_ref.repo.clone(),
            path: action_ref.path.clone(),
            ref_name: action_ref.ref_name.clone(),
        },
        message: "Action is pinned to a mutable tag".to_string(),
        recommendation: "Pin to a full SHA".to_string(),
        fixable: true,
        expected_sha: None,
    }
}

fn is_full_sha(ref_name: &str) -> bool {
    ref_name.len() == 40 && ref_name.chars().all(|character| character.is_ascii_hexdigit())
}
