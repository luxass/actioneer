use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Action {
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

impl Action {
    #[allow(clippy::too_many_arguments)]
    pub fn from_scan(
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

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub sha: String,
    pub version: Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum PinStyle {
    #[default]
    Sha,
    Tag,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum UpdateMode {
    #[default]
    Major,
    Minor,
    Patch,
}

#[derive(Debug, Clone)]
pub struct ResolveConfig {
    pub excludes: Vec<String>,
    pub skip_branches: bool,
    pub mode: UpdateMode,
    pub style: PinStyle,
}

pub fn parse_version(raw: &str) -> Option<Version> {
    let value = raw
        .strip_prefix('v')
        .or_else(|| raw.strip_prefix('V'))
        .unwrap_or(raw);
    if value.is_empty() || !value.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let mut parts = value.split('.');
    let major = parse_leading_int(parts.next()?)?;
    let minor = parse_leading_int(parts.next().unwrap_or("0"))?;
    let patch = parse_leading_int(parts.next().unwrap_or("0"))?;
    Some(Version {
        major,
        minor,
        patch,
    })
}

fn parse_leading_int(value: &str) -> Option<u32> {
    let end = value.bytes().take_while(|b| b.is_ascii_digit()).count();
    if end == 0 {
        return None;
    }
    value[..end].parse().ok()
}

pub fn is_likely_sha(value: &str) -> bool {
    (7..=40).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn sha_matches(actual: &str, expected: &str) -> bool {
    actual == expected || expected.starts_with(actual)
}
