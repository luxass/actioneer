use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ActionReference {
    pub owner: String,
    pub name: String,
    pub path: String,
    pub current_ref: String,
    pub version_comment: Option<String>,
    pub file: String,
    pub line: usize,
    #[serde(skip)]
    pub edit: WorkflowEdit,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionUpdate {
    #[serde(flatten)]
    pub action: ActionReference,
    pub new_ref: String,
    pub new_version: String,
    pub expected_sha: String,
    pub sha_mismatch: bool,
    pub is_branch: bool,
    pub is_major: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UpdateNote {
    ShaMismatch,
    MutableBranch,
    MajorUpdate,
}

#[derive(Debug, Clone)]
pub struct WorkflowEdit {
    pub(crate) ref_start: usize,
    pub(crate) ref_end: usize,
}

impl WorkflowEdit {
    pub fn new(ref_start: usize, ref_end: usize) -> Self {
        Self { ref_start, ref_end }
    }
}

impl ActionReference {
    pub fn action_name(&self) -> String {
        format!("{}/{}{}", self.owner, self.name, self.path)
    }
}

impl ActionUpdate {
    pub fn action_name(&self) -> String {
        self.action.action_name()
    }

    pub fn version_label(&self) -> String {
        let current = self
            .action
            .version_comment
            .as_deref()
            .unwrap_or(&self.action.current_ref);
        if current == self.new_version {
            self.new_version.clone()
        } else {
            format!("{} -> {}", current, self.new_version)
        }
    }

    pub fn notes(&self) -> Vec<UpdateNote> {
        let mut notes = Vec::new();
        if self.sha_mismatch {
            notes.push(UpdateNote::ShaMismatch);
        }
        if self.is_branch {
            notes.push(UpdateNote::MutableBranch);
        }
        if self.is_major {
            notes.push(UpdateNote::MajorUpdate);
        }
        notes
    }

    pub fn should_write_version_comment(&self) -> bool {
        !self.new_version.is_empty()
            && (self.ref_differs_from_version()
                || self.action.version_comment.is_some()
                || self.sha_mismatch)
    }

    pub fn ref_differs_from_version(&self) -> bool {
        self.new_ref != self.new_version
    }

    pub fn is_security_sensitive(&self) -> bool {
        self.sha_mismatch || self.is_branch
    }
}
