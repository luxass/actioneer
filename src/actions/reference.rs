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
    pub(crate) ref_start: usize,
    #[serde(skip)]
    pub(crate) ref_end: usize,
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

impl ActionReference {
    #[allow(clippy::too_many_arguments)]
    pub fn from_discovery(
        owner: String,
        name: String,
        path: String,
        current_ref: String,
        version_comment: Option<String>,
        file: String,
        line: usize,
        ref_start: usize,
        ref_end: usize,
    ) -> Self {
        Self {
            owner,
            name,
            path,
            current_ref,
            version_comment,
            file,
            line,
            ref_start,
            ref_end,
        }
    }

    pub fn action_name(&self) -> String {
        format!("{}/{}{}", self.owner, self.name, self.path)
    }
}

impl ActionUpdate {
    pub fn action_name(&self) -> String {
        self.action.action_name()
    }
}
