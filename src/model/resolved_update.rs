use serde::Serialize;

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
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedUpdate {
    /// Repository or reusable-workflow name, such as `actions/checkout`.
    pub action: String,
    /// Job or composite scope used for display in prompts and logs.
    pub job: String,
    /// The exact current ref text from the `uses:` clause.
    pub current: String,
    /// The resolved SHA that the current ref is expected to point to, when known.
    #[serde(skip_serializing)]
    pub current_ref: String,
    /// Parsed version comment from the source line, for example `v4.2.0`.
    #[serde(rename = "versionComment")]
    pub version_comment: String,
    /// Whether the current SHA conflicts with the stated version comment.
    #[serde(rename = "shaMismatch")]
    pub sha_mismatch: bool,
    /// The exact replacement text that should be written into the `uses:` ref slot.
    pub next: String,
    /// Human-friendly label for the replacement target, usually the matching tag.
    #[serde(rename = "nextLabel", serialize_with = "serialize_next_label")]
    pub next_label: String,
    /// File containing the reference.
    pub file: String,
    /// 1-based source line for display.
    pub line: usize,
    /// Whether this update crosses a major version boundary.
    #[serde(skip_serializing)]
    pub next_is_major: bool,
    /// Start byte of the ref portion inside the source file.
    #[serde(skip_serializing)]
    pub ref_start: usize,
    /// End byte of the ref portion inside the source file.
    #[serde(skip_serializing)]
    pub ref_end: usize,
}

fn serialize_next_label<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(value)
}

impl ResolvedUpdate {
    pub fn display_target(&self) -> &str {
        if self.next_label.is_empty() {
            &self.next
        } else {
            &self.next_label
        }
    }

    pub fn has_current_ref(&self) -> bool {
        !self.current_ref.is_empty()
    }

    pub fn has_version_comment(&self) -> bool {
        !self.version_comment.is_empty()
    }

    pub fn has_sha_mismatch(&self) -> bool {
        self.sha_mismatch
    }

    pub fn is_major_update(&self) -> bool {
        self.next_is_major
    }

    pub fn should_write_version_comment(&self) -> bool {
        let target = self.display_target();
        !target.is_empty()
            && (self.next != target || self.has_version_comment() || self.has_sha_mismatch())
    }
}
