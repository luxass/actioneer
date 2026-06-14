use std::path::Path;

use actioneer::discovery::discover_action_refs;

#[test]
fn discovers_external_workflow_uses_and_ignores_local_and_docker_refs() {
    let refs = discover_action_refs([Path::new(
        "testdata/workflows/discovery/ignores-local-and-docker",
    )])
    .expect("discover action refs");

    let actions = refs
        .iter()
        .map(|action| {
            (
                action.repo.as_str(),
                action.path.as_str(),
                action.ref_name.as_str(),
                action.line,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actions,
        vec![
            ("luxass/reusable", ".github/workflows/ci.yml", "v1", 8),
            ("actions/checkout", "", "v4", 13),
            ("owner/tool", "path", "main", 14),
        ]
    );

    assert!(refs.iter().all(|action| action.file.ends_with(
        ".github/workflows/ci.yml"
    )));
}
