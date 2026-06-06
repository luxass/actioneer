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
    pub ref_start: usize,
    pub ref_end: usize,
    pub new_ref: String,
    pub new_version: String,
    pub expected_sha: String,
    pub sha_mismatch: bool,
    pub is_branch: bool,
    pub is_major: bool,
    pub needs_update: bool,
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
            new_ref: String::new(),
            new_version: String::new(),
            expected_sha: String::new(),
            sha_mismatch: false,
            is_branch: false,
            is_major: false,
            needs_update: false,
        }
    }

    pub fn action_name(&self) -> String {
        format!("{}/{}{}", self.owner, self.name, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
