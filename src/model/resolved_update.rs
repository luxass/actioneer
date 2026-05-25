use serde::ser::{Serialize, SerializeStruct, Serializer};

/// A fully resolved workflow update that can be shown to the user and applied to a file.
///
/// The parser produces `Reference` values from raw `uses:` entries. The resolver then turns each
/// updatable reference into a `ResolvedUpdate` by deciding:
/// - what the current ref means
/// - whether the current SHA/comment pair is inconsistent
/// - what the next ref should be
/// - which byte range in the source file needs rewriting
///
/// This makes `ResolvedUpdate` a boundary object between three parts of the program:
/// - resolver: computes update targets and validation state
/// - commands/output: renders the proposed update
/// - rewrite: applies the chosen replacement back to the file
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedUpdate {
    pub action: String,
    pub job: String,
    pub current: String,
    pub validation: ValidationState,
    pub target: UpdateTarget,
    pub source: UpdateSource,
    pub is_branch_ref: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationState {
    expected_sha: String,
    version_comment: String,
    sha_mismatch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    next_ref: String,
    display_name: String,
    is_major: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateSource {
    file: String,
    line: usize,
    ref_start: usize,
    ref_end: usize,
}

impl ResolvedUpdate {
    pub fn new(
        action: impl Into<String>,
        job: impl Into<String>,
        current: impl Into<String>,
        validation: ValidationState,
        target: UpdateTarget,
        source: UpdateSource,
        is_branch_ref: bool,
    ) -> Self {
        Self {
            action: action.into(),
            job: job.into(),
            current: current.into(),
            validation,
            target,
            source,
            is_branch_ref,
        }
    }

    pub fn display_target(&self) -> &str {
        self.target.display_target()
    }

    pub fn next_ref(&self) -> &str {
        self.target.next_ref()
    }

    pub fn has_current_ref(&self) -> bool {
        self.validation.has_expected_sha()
    }

    pub fn current_ref(&self) -> &str {
        self.validation.expected_sha()
    }

    pub fn has_version_comment(&self) -> bool {
        self.validation.has_version_comment()
    }

    pub fn version_comment(&self) -> &str {
        self.validation.version_comment()
    }

    pub fn has_sha_mismatch(&self) -> bool {
        self.validation.sha_mismatch()
    }

    pub fn is_major_update(&self) -> bool {
        self.target.is_major()
    }

    pub fn file(&self) -> &str {
        self.source.file()
    }

    pub fn line(&self) -> usize {
        self.source.line()
    }

    pub fn display_current(&self) -> &str {
        if self.has_current_ref() && !self.validation.version_comment().is_empty() {
            self.validation.version_comment()
        } else {
            &self.current
        }
    }

    pub fn ref_start(&self) -> usize {
        self.source.ref_start()
    }

    pub fn ref_end(&self) -> usize {
        self.source.ref_end()
    }

    pub fn is_branch_ref(&self) -> bool {
        self.is_branch_ref
    }

    pub fn should_write_version_comment(&self) -> bool {
        let target = self.display_target();
        !target.is_empty()
            && (self.next_ref() != target || self.has_version_comment() || self.has_sha_mismatch())
    }
}

impl ValidationState {
    pub fn new(
        expected_sha: impl Into<String>,
        version_comment: impl Into<String>,
        sha_mismatch: bool,
    ) -> Self {
        Self {
            expected_sha: expected_sha.into(),
            version_comment: version_comment.into(),
            sha_mismatch,
        }
    }

    pub fn expected_sha(&self) -> &str {
        &self.expected_sha
    }

    pub fn has_expected_sha(&self) -> bool {
        !self.expected_sha.is_empty()
    }

    pub fn version_comment(&self) -> &str {
        &self.version_comment
    }

    pub fn has_version_comment(&self) -> bool {
        !self.version_comment.is_empty()
    }

    pub fn sha_mismatch(&self) -> bool {
        self.sha_mismatch
    }
}

impl UpdateTarget {
    pub fn new(
        next_ref: impl Into<String>,
        display_name: impl Into<String>,
        is_major: bool,
    ) -> Self {
        Self {
            next_ref: next_ref.into(),
            display_name: display_name.into(),
            is_major,
        }
    }

    pub fn next_ref(&self) -> &str {
        &self.next_ref
    }

    pub fn display_target(&self) -> &str {
        if self.display_name.is_empty() {
            &self.next_ref
        } else {
            &self.display_name
        }
    }

    pub fn is_major(&self) -> bool {
        self.is_major
    }
}

impl UpdateSource {
    pub fn new(file: impl Into<String>, line: usize, ref_start: usize, ref_end: usize) -> Self {
        Self {
            file: file.into(),
            line,
            ref_start,
            ref_end,
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn ref_start(&self) -> usize {
        self.ref_start
    }

    pub fn ref_end(&self) -> usize {
        self.ref_end
    }
}

impl Serialize for ResolvedUpdate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ResolvedUpdate", 9)?;
        state.serialize_field("action", &self.action)?;
        state.serialize_field("job", &self.job)?;
        state.serialize_field("current", &self.current)?;
        state.serialize_field("versionComment", self.version_comment())?;
        state.serialize_field("shaMismatch", &self.has_sha_mismatch())?;
        state.serialize_field("isBranchRef", &self.is_branch_ref)?;
        state.serialize_field("next", self.next_ref())?;
        state.serialize_field("nextLabel", self.display_target())?;
        state.serialize_field("file", self.file())?;
        state.serialize_field("line", &self.line())?;
        state.end()
    }
}
