use actioneer::actions::{ActionReference, ActionUpdate, Tag, Version, WorkflowEdit};

pub fn reference() -> ActionReference {
    ActionReference {
        owner: "actions".into(),
        name: "checkout".into(),
        path: String::new(),
        current_ref: "v1.0.0".into(),
        version_comment: None,
        file: "ci.yml".into(),
        line: 1,
        edit: WorkflowEdit::new(0, 6),
    }
}

pub fn update(action: ActionReference) -> ActionUpdate {
    ActionUpdate {
        action,
        new_ref: "newref".into(),
        new_version: "v2.0.0".into(),
        expected_sha: String::new(),
        sha_mismatch: false,
        is_branch: false,
        is_major: false,
    }
}

pub fn tag(name: &str, sha: &str, major: u32, minor: u32, patch: u32) -> Tag {
    Tag {
        name: name.to_string(),
        sha: sha.to_string(),
        version: Version {
            major,
            minor,
            patch,
        },
    }
}
