use std::fmt::{self, Display};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReferenceKind {
    WorkflowJob,
    WorkflowStep,
    CompositeStep,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

impl Repository {
    pub fn display(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ActionName {
    pub repository: Repository,
    pub path: String,
}

impl ActionName {
    pub fn display(&self) -> String {
        format!(
            "{}/{}{}",
            self.repository.owner, self.repository.name, self.path
        )
    }
}

impl Display for ActionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// Byte offsets for the mutable ref portion inside the source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

/// Concrete source location for a parsed `uses:` reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub ref_span: ByteSpan,
}

/// Parsed GitHub-hosted `uses:` reference extracted from a workflow or composite action file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub name: ActionName,
    pub current_ref: String,
    pub version_hint: String,
    pub scope: String,
    pub source: SourceLocation,
}
