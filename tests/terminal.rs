use actioneer::actions::{ActionReference, ActionUpdate};
use actioneer::terminal::display::{short_sha, update_file_count};

#[path = "support/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;

fn action(file: &str, name: &str) -> ActionUpdate {
    fixtures::update(ActionReference {
        name: name.into(),
        file: file.into(),
        ..fixtures::reference()
    })
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
