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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_v_prefix() {
        let v = parse_version("v1.2.3").unwrap();
        assert_eq!(Version { major: 1, minor: 2, patch: 3 }, v);
    }

    #[test]
    fn parse_version_capital_v() {
        let v = parse_version("V4.5.6").unwrap();
        assert_eq!(Version { major: 4, minor: 5, patch: 6 }, v);
    }

    #[test]
    fn parse_version_no_prefix() {
        let v = parse_version("7.8.9").unwrap();
        assert_eq!(Version { major: 7, minor: 8, patch: 9 }, v);
    }

    #[test]
    fn parse_version_major_only() {
        let v = parse_version("v1").unwrap();
        assert_eq!(Version { major: 1, minor: 0, patch: 0 }, v);
    }

    #[test]
    fn parse_version_major_minor() {
        let v = parse_version("v1.2").unwrap();
        assert_eq!(Version { major: 1, minor: 2, patch: 0 }, v);
    }

    #[test]
    fn parse_version_empty_returns_none() {
        assert!(parse_version("").is_none());
    }

    #[test]
    fn parse_version_bare_v_returns_none() {
        assert!(parse_version("v").is_none());
    }

    #[test]
    fn parse_version_non_numeric_returns_none() {
        assert!(parse_version("not-a-version").is_none());
    }

    #[test]
    fn parse_version_trailing_text_parses_leading_digits() {
        let v = parse_version("v1.2.3-beta").unwrap();
        assert_eq!(Version { major: 1, minor: 2, patch: 3 }, v);
    }

    #[test]
    fn parse_version_leading_zero() {
        let v = parse_version("v0.1.0").unwrap();
        assert_eq!(Version { major: 0, minor: 1, patch: 0 }, v);
    }

    #[test]
    fn is_likely_sha_shortest_valid() {
        assert!(is_likely_sha("abcdef0"));
    }

    #[test]
    fn is_likely_sha_longest_valid() {
        assert!(is_likely_sha("abcdef0123456789abcdef0123456789abcdef01"));
    }

    #[test]
    fn is_likely_sha_too_short() {
        assert!(!is_likely_sha("abcde"));
    }

    #[test]
    fn is_likely_sha_too_long() {
        assert!(!is_likely_sha("abcdef0123456789abcdef0123456789abcdef0123"));
    }

    #[test]
    fn is_likely_sha_non_hex() {
        assert!(!is_likely_sha("abcdefg"));
    }

    #[test]
    fn is_likely_sha_empty() {
        assert!(!is_likely_sha(""));
    }

    #[test]
    fn sha_matches_exact() {
        assert!(sha_matches("abc123", "abc123"));
    }

    #[test]
    fn sha_matches_prefix() {
        assert!(sha_matches("abc", "abc123456789"));
    }

    #[test]
    fn sha_matches_mismatch() {
        assert!(!sha_matches("abc", "def456"));
    }

    #[test]
    fn action_name_no_path() {
        let a = Action::from_scan("own".into(), "repo".into(), String::new(), "v1".into(), None, "f".into(), 1, 0, 2);
        assert_eq!("own/repo", a.action_name());
    }

    #[test]
    fn action_name_with_path() {
        let a = Action::from_scan("own".into(), "repo".into(), "/.github/workflows/ci.yml".into(), "v1".into(), None, "f".into(), 1, 0, 2);
        assert_eq!("own/repo/.github/workflows/ci.yml", a.action_name());
    }

    #[test]
    fn version_ordering_major() {
        assert!(Version { major: 2, minor: 0, patch: 0 } > Version { major: 1, minor: 9, patch: 9 });
    }

    #[test]
    fn version_ordering_minor() {
        assert!(Version { major: 1, minor: 3, patch: 0 } > Version { major: 1, minor: 2, patch: 9 });
    }

    #[test]
    fn version_ordering_patch() {
        assert!(Version { major: 1, minor: 2, patch: 5 } > Version { major: 1, minor: 2, patch: 4 });
    }

    #[test]
    fn version_ordering_equal() {
        assert_eq!(Version { major: 1, minor: 2, patch: 3 }, Version { major: 1, minor: 2, patch: 3 });
    }
}
