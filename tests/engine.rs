use actioneer::engine::{comment_matches_ref, parse_workflow, ActionReference, AuditTier, CommentMatch, PinKind, ReferenceKind};

// Helpers

fn workflow(name: &str) -> String {
    let path = format!(
        "{}/testdata/workflows/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

// Basic workflow

#[test]
fn basic_document_name() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    assert_eq!(doc.name.as_deref(), Some("CI"));
}

#[test]
fn basic_reference_count() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    // build: checkout + setup-node (run step has no uses)
    // lint: checkout + setup-node  → 4 total
    assert_eq!(doc.references.len(), 4);
}

#[test]
fn basic_first_ref_metadata() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    let r = &doc.references[0];
    assert_eq!(r.raw, "actions/checkout@v4");
    assert_eq!(r.kind, ReferenceKind::Action);
    assert_eq!(r.pin_kind, PinKind::Tag);
    assert_eq!(r.owner.as_deref(), Some("actions"));
    assert_eq!(r.repo.as_deref(), Some("checkout"));
    assert_eq!(r.git_ref.as_deref(), Some("v4"));
    assert!(r.subpath.is_none());
    assert_eq!(r.step_name.as_deref(), Some("Checkout"));
    assert_eq!(r.job_id, "build");
    assert_eq!(r.job_name.as_deref(), Some("Build and test"));
    assert_eq!(r.step_index, Some(0));
}

#[test]
fn basic_run_step_not_included() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    assert!(!doc.references.iter().any(|r| r.raw.is_empty()));
}

#[test]
fn basic_step_indices() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    let build_refs: Vec<_> = doc.references.iter().filter(|r| r.job_id == "build").collect();
    assert_eq!(build_refs[0].step_index, Some(0)); // checkout
    assert_eq!(build_refs[1].step_index, Some(1)); // setup-node
}

#[test]
fn basic_step_without_name() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    // The lint job's first step has no `name:` field.
    let lint_steps: Vec<_> = doc.references.iter().filter(|r| r.job_id == "lint").collect();
    assert!(lint_steps[0].step_name.is_none());
}

#[test]
fn basic_job_without_name() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    let lint: Vec<_> = doc.references.iter().filter(|r| r.job_id == "lint").collect();
    // The lint job has no `name:` field.
    assert!(lint[0].job_name.is_none());
}

#[test]
fn basic_line_numbers_assigned() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    for r in &doc.references {
        assert!(
            r.line.is_some(),
            "expected line number for {}, got None",
            r.raw
        );
    }
}

#[test]
fn basic_document_order() {
    let doc = parse_workflow(&workflow("basic.yml")).unwrap();
    let raws: Vec<&str> = doc.references.iter().map(|r| r.raw.as_str()).collect();
    assert_eq!(
        raws,
        &[
            "actions/checkout@v4",
            "actions/setup-node@v4",
            "actions/checkout@v4",
            "actions/setup-node@v3",
        ]
    );
}

// No-name workflow

#[test]
fn no_name_document() {
    let doc = parse_workflow(&workflow("no_name.yml")).unwrap();
    assert!(doc.name.is_none());
    assert_eq!(doc.references.len(), 1);
}

// Pinned SHA workflow

#[test]
fn pinned_full_sha() {
    let doc = parse_workflow(&workflow("pinned.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675")
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::FullSha);
    assert_eq!(
        r.git_ref.as_deref(),
        Some("a81bbbf8298c0fa03ea29cdc473d45769f953675")
    );
}

#[test]
fn pinned_short_sha() {
    let doc = parse_workflow(&workflow("pinned.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw.contains("1bd1e32"))
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::ShortSha);
}

#[test]
fn pinned_tag() {
    let doc = parse_workflow(&workflow("pinned.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/setup-python@v5")
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::Tag);
}

#[test]
fn pinned_branch() {
    let doc = parse_workflow(&workflow("pinned.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "some-org/some-action@main")
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::Branch);
}

// Advanced workflow

#[test]
fn advanced_job_level_reusable_workflow() {
    let doc = parse_workflow(&workflow("advanced.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.job_id == "reusable-job")
        .unwrap();
    assert_eq!(
        r.raw,
        "octo-org/octo-repo/.github/workflows/workflow.yml@v1"
    );
    assert_eq!(r.kind, ReferenceKind::ReusableWorkflow);
    assert_eq!(r.pin_kind, PinKind::Tag);
    assert_eq!(r.owner.as_deref(), Some("octo-org"));
    assert_eq!(r.repo.as_deref(), Some("octo-repo"));
    assert_eq!(
        r.subpath.as_deref(),
        Some(".github/workflows/workflow.yml")
    );
    assert!(r.step_index.is_none(), "job-level uses should have no step_index");
    assert!(r.step_name.is_none());
}

#[test]
fn advanced_local_reusable_workflow_job_level() {
    let doc = parse_workflow(&workflow("advanced.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.job_id == "local-reusable")
        .unwrap();
    assert_eq!(r.kind, ReferenceKind::ReusableWorkflow);
    assert_eq!(r.pin_kind, PinKind::Unpinned);
    assert!(r.owner.is_none());
    assert!(r.repo.is_none());
    assert_eq!(r.subpath.as_deref(), Some("./.github/workflows/deploy.yml"));
}

#[test]
fn advanced_docker_image() {
    let doc = parse_workflow(&workflow("advanced.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "docker://alpine:3.14")
        .unwrap();
    assert_eq!(r.kind, ReferenceKind::Docker);
    assert_eq!(r.pin_kind, PinKind::Unpinned);
    assert_eq!(r.subpath.as_deref(), Some("alpine:3.14"));
    assert!(r.git_ref.is_none());
    assert!(r.owner.is_none());
}

#[test]
fn advanced_local_action() {
    let doc = parse_workflow(&workflow("advanced.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "./my-local-action")
        .unwrap();
    assert_eq!(r.kind, ReferenceKind::LocalAction);
    assert_eq!(r.pin_kind, PinKind::Unpinned);
    assert!(r.owner.is_none());
    assert!(r.repo.is_none());
    assert_eq!(r.subpath.as_deref(), Some("./my-local-action"));
}

#[test]
fn advanced_nested_subpath_action() {
    let doc = parse_workflow(&workflow("advanced.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/aws-actions/amazon-ecr-login@v2")
        .unwrap();
    assert_eq!(r.kind, ReferenceKind::Action);
    assert_eq!(r.owner.as_deref(), Some("actions"));
    assert_eq!(r.repo.as_deref(), Some("aws-actions"));
    assert_eq!(r.subpath.as_deref(), Some("amazon-ecr-login"));
    assert_eq!(r.pin_kind, PinKind::Tag);
}

// Inline YAML tests (no fixture files needed)

#[test]
fn empty_jobs_returns_empty_references() {
    let yaml = "name: Empty\non: push\njobs: {}\n";
    let doc = parse_workflow(yaml).unwrap();
    assert!(doc.references.is_empty());
}

#[test]
fn job_with_only_run_steps_no_references() {
    let yaml = indoc(
        "
        name: Run only
        on: push
        jobs:
          test:
            runs-on: ubuntu-latest
            steps:
              - run: echo hello
              - run: cargo test
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    assert!(doc.references.is_empty());
}

#[test]
fn invalid_yaml_returns_error() {
    let result = parse_workflow(": bad: yaml: ][");
    assert!(result.is_err());
}

#[test]
fn pin_kind_display() {
    assert_eq!(PinKind::FullSha.to_string(), "full-sha");
    assert_eq!(PinKind::ShortSha.to_string(), "short-sha");
    assert_eq!(PinKind::Tag.to_string(), "tag");
    assert_eq!(PinKind::Branch.to_string(), "branch");
    assert_eq!(PinKind::Unpinned.to_string(), "unpinned");
}

#[test]
fn reference_kind_display() {
    assert_eq!(ReferenceKind::Action.to_string(), "action");
    assert_eq!(ReferenceKind::LocalAction.to_string(), "local");
    assert_eq!(ReferenceKind::Docker.to_string(), "docker");
    assert_eq!(ReferenceKind::ReusableWorkflow.to_string(), "reusable-workflow");
}

#[test]
fn single_step_line_number() {
    let yaml = indoc(
        "
        on: push
        jobs:
          test:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    assert_eq!(doc.references.len(), 1);
    // Line 6: `      - uses: actions/checkout@v4`
    assert_eq!(doc.references[0].line, Some(6));
}

// Comment extraction - inline YAML

#[test]
fn comment_extracted_from_uses_line() {
    let yaml = indoc(
        "
        on: push
        jobs:
          build:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675 # v4.2.0
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    assert_eq!(doc.references.len(), 1);
    assert_eq!(
        doc.references[0].line_comment.as_deref(),
        Some("v4.2.0")
    );
}

#[test]
fn comment_none_when_absent() {
    let yaml = indoc(
        "
        on: push
        jobs:
          build:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    assert!(doc.references[0].line_comment.is_none());
}

#[test]
fn comment_none_for_empty_hash() {
    let yaml = indoc(
        "
        on: push
        jobs:
          build:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4 #
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    assert!(doc.references[0].line_comment.is_none());
}

#[test]
fn comment_and_line_both_assigned() {
    let yaml = indoc(
        "
        on: push
        jobs:
          build:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4 # v4
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    let r = &doc.references[0];
    assert!(r.line.is_some(), "line must be set");
    assert_eq!(r.line_comment.as_deref(), Some("v4"));
}

#[test]
fn comment_uses_line_also_sets_correct_line_number() {
    let yaml = indoc(
        "
        on: push
        jobs:
          build:
            runs-on: ubuntu-latest
            steps:
              - uses: actions/checkout@v4 # v4.2.0
        ",
    );
    let doc = parse_workflow(&yaml).unwrap();
    // Line 6: `- uses: actions/checkout@v4 # v4.2.0`
    assert_eq!(doc.references[0].line, Some(6));
}

// Comment extraction - fixture file

#[test]
fn comments_fixture_reference_count() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    assert_eq!(doc.references.len(), 6);
}

#[test]
fn comments_fixture_sha_pin_with_tag_comment() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/checkout@a81bbbf8298c0fa03ea29cdc473d45769f953675")
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::FullSha);
    assert_eq!(r.line_comment.as_deref(), Some("v4.2.0"));
}

#[test]
fn comments_fixture_tag_pin_with_matching_comment() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/setup-node@v4")
        .unwrap();
    assert_eq!(r.pin_kind, PinKind::Tag);
    assert_eq!(r.line_comment.as_deref(), Some("v4"));
}

#[test]
fn comments_fixture_tag_pin_with_mismatched_comment() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/setup-python@v5")
        .unwrap();
    assert_eq!(r.line_comment.as_deref(), Some("v4"));
}

#[test]
fn comments_fixture_sha_pin_with_sha_comment() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/cache@3624ceb22c1c005a02ab054990a4879e89888bcd")
        .unwrap();
    assert_eq!(
        r.line_comment.as_deref(),
        Some("3624ceb22c1c005a02ab054990a4879e89888bcd")
    );
}

#[test]
fn comments_fixture_no_comment() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/upload-artifact@v4")
        .unwrap();
    assert!(r.line_comment.is_none());
}

#[test]
fn comments_fixture_empty_hash_is_none() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    let r = doc
        .references
        .iter()
        .find(|r| r.raw == "actions/download-artifact@v4")
        .unwrap();
    assert!(r.line_comment.is_none());
}

#[test]
fn comments_fixture_all_line_numbers_assigned() {
    let doc = parse_workflow(&workflow("comments.yml")).unwrap();
    for r in &doc.references {
        assert!(
            r.line.is_some(),
            "expected line number for {}, got None",
            r.raw
        );
    }
}

// CommentMatch - unit-style tests using inline ActionReference construction

fn make_ref(raw: &str, git_ref: Option<&str>, pin_kind: PinKind, comment: Option<&str>) -> ActionReference {
    ActionReference {
        raw: raw.into(),
        kind: ReferenceKind::Action,
        pin_kind,
        owner: Some("actions".into()),
        repo: Some("checkout".into()),
        subpath: None,
        git_ref: git_ref.map(str::to_string),
        step_name: None,
        job_id: "build".into(),
        job_name: None,
        step_index: Some(0),
        line: Some(1),
        line_comment: comment.map(str::to_string),
    }
}

#[test]
fn comment_match_no_comment() {
    let r = make_ref("actions/checkout@v4", Some("v4"), PinKind::Tag, None);
    assert_eq!(comment_matches_ref(&r), CommentMatch::NoComment);
}

#[test]
fn comment_match_tag_exact_match() {
    let r = make_ref("actions/checkout@v4", Some("v4"), PinKind::Tag, Some("v4"));
    assert_eq!(comment_matches_ref(&r), CommentMatch::Match);
}

#[test]
fn comment_match_tag_with_version_string() {
    let r = make_ref(
        "actions/checkout@v4.2.0",
        Some("v4.2.0"),
        PinKind::Tag,
        Some("v4.2.0"),
    );
    assert_eq!(comment_matches_ref(&r), CommentMatch::Match);
}

#[test]
fn comment_match_tag_mismatch() {
    let r = make_ref(
        "actions/setup-python@v5",
        Some("v5"),
        PinKind::Tag,
        Some("v4"),
    );
    assert_eq!(
        comment_matches_ref(&r),
        CommentMatch::Mismatch {
            comment: "v4".into(),
            expected: "v5".into(),
        }
    );
}

#[test]
fn comment_match_full_sha_tag_comment_is_mismatch() {
    // SHA pin with a tag comment - the comment is "v4.2.0" but the ref is the SHA.
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let r = make_ref(
        &format!("actions/checkout@{sha}"),
        Some(sha),
        PinKind::FullSha,
        Some("v4.2.0"),
    );
    assert_eq!(
        comment_matches_ref(&r),
        CommentMatch::Mismatch {
            comment: "v4.2.0".into(),
            expected: sha.into(),
        }
    );
}

#[test]
fn comment_match_full_sha_sha_comment_is_match() {
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let r = make_ref(
        &format!("actions/checkout@{sha}"),
        Some(sha),
        PinKind::FullSha,
        Some(sha),
    );
    assert_eq!(comment_matches_ref(&r), CommentMatch::Match);
}

#[test]
fn comment_match_sha_embedded_in_comment_text() {
    // Comment contains more text around the SHA, e.g. a prose note.
    let sha = "3624ceb22c1c005a02ab054990a4879e89888bcd";
    let r = make_ref(
        &format!("actions/cache@{sha}"),
        Some(sha),
        PinKind::FullSha,
        Some(&format!("pinned to {sha} by renovate")),
    );
    assert_eq!(comment_matches_ref(&r), CommentMatch::Match);
}

#[test]
fn comment_match_no_git_ref_is_mismatch() {
    // Comment present but no git_ref (e.g. a local action somehow got a comment).
    let r = ActionReference {
        raw: "./my-action".into(),
        kind: ReferenceKind::LocalAction,
        pin_kind: PinKind::Unpinned,
        owner: None,
        repo: None,
        subpath: Some("./my-action".into()),
        git_ref: None,
        step_name: None,
        job_id: "build".into(),
        job_name: None,
        step_index: Some(0),
        line: Some(5),
        line_comment: Some("some comment".into()),
    };
    assert_eq!(
        comment_matches_ref(&r),
        CommentMatch::Mismatch {
            comment: "some comment".into(),
            expected: String::new(),
        }
    );
}

// AuditTier and is_updatable

#[test]
fn audit_tier_action_is_primary() {
    assert_eq!(ReferenceKind::Action.audit_tier(), AuditTier::Primary);
}

#[test]
fn audit_tier_reusable_workflow_is_primary() {
    assert_eq!(ReferenceKind::ReusableWorkflow.audit_tier(), AuditTier::Primary);
}

#[test]
fn audit_tier_docker_is_secondary() {
    assert_eq!(ReferenceKind::Docker.audit_tier(), AuditTier::Secondary);
}

#[test]
fn audit_tier_local_action_is_secondary() {
    assert_eq!(ReferenceKind::LocalAction.audit_tier(), AuditTier::Secondary);
}

#[test]
fn is_updatable_action_true() {
    assert!(ReferenceKind::Action.is_updatable());
}

#[test]
fn is_updatable_reusable_workflow_false() {
    assert!(!ReferenceKind::ReusableWorkflow.is_updatable());
}

#[test]
fn is_updatable_docker_false() {
    assert!(!ReferenceKind::Docker.is_updatable());
}

#[test]
fn is_updatable_local_action_false() {
    assert!(!ReferenceKind::LocalAction.is_updatable());
}

/// Minimal indentation helper so we can write multi-line YAML inline.
fn indoc(s: &str) -> String {
    let stripped = s.trim_start_matches('\n');
    // Find minimum leading whitespace
    let min_indent = stripped
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    stripped
        .lines()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
