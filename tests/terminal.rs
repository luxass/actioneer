use actioneer::actions::{ActionReference, ActionUpdate};
use actioneer::terminal::display::{short_sha, update_file_count};

fn action(file: &str, name: &str) -> ActionUpdate {
    ActionUpdate {
        action: ActionReference::from_discovery(
            "o".into(),
            name.into(),
            String::new(),
            "v1".into(),
            None,
            file.into(),
            1,
            0,
            2,
        ),
        new_ref: "sha".into(),
        new_version: "v1".into(),
        expected_sha: String::new(),
        sha_mismatch: false,
        is_branch: false,
        is_major: false,
    }
}

#[test]
fn short_sha_truncates_long_values() {
    assert_eq!(&"abcdef0123456789"[..12], short_sha("abcdef0123456789"));
}

#[test]
fn short_sha_preserves_short_values() {
    assert_eq!("abcdef012345", short_sha("abcdef012345"));
    assert_eq!("abc", short_sha("abc"));
}

#[test]
fn update_file_count_counts_unique_files() {
    assert_eq!(0, update_file_count(&[]));
    assert_eq!(
        1,
        update_file_count(&[action("ci.yml", "n"), action("ci.yml", "n2")])
    );
    assert_eq!(
        2,
        update_file_count(&[action("a.yml", "n"), action("b.yml", "n2")])
    );
}
