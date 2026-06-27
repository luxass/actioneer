use actioneer::cache::resolve_cache_dir_with;

#[test]
fn env_override_used_when_set() {
    let dir = resolve_cache_dir_with(Some("/tmp/my-actioneer-cache")).unwrap();
    assert_eq!(dir.path().to_str().unwrap(), "/tmp/my-actioneer-cache");
}

#[test]
fn env_override_ignored_when_empty() {
    // An empty string means "not set"; should fall back to the platform cache dir.
    let from_empty = resolve_cache_dir_with(Some(""));
    let from_none = resolve_cache_dir_with(None);

    // Both should agree (either both None, or both point to the same path).
    assert_eq!(from_empty, from_none);
}

#[test]
fn platform_fallback_ends_with_actioneer() {
    if let Some(dir) = resolve_cache_dir_with(None) {
        assert!(
            dir.path().ends_with("actioneer"),
            "expected path to end with 'actioneer', got {:?}",
            dir.path()
        );
    }
    // If no home/cache directory is available (unusual in CI), skip the assertion.
}

#[test]
fn cache_dir_as_ref_path() {
    let dir = resolve_cache_dir_with(Some("/tmp/test-cache")).unwrap();
    let as_ref: &std::path::Path = dir.as_ref();
    assert_eq!(as_ref, dir.path());
}
