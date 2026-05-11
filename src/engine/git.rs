#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub fn is_likely_sha(value: &str) -> bool {
    (7..=40).contains(&value.len()) && value.bytes().all(|char| char.is_ascii_hexdigit())
}

pub fn sha_matches(actual: &str, expected: &str) -> bool {
    actual == expected || expected.starts_with(actual)
}

pub fn parse_version(reference: &str) -> Option<Version> {
    let value = reference
        .strip_prefix('v')
        .or_else(|| reference.strip_prefix('V'))
        .unwrap_or(reference);
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
    let end = value
        .bytes()
        .take_while(|char| char.is_ascii_digit())
        .count();
    if end == 0 {
        return None;
    }
    value[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_likely_git_sha() {
        assert!(is_likely_sha("123abcd"));
        assert!(is_likely_sha("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_likely_sha("123abc"));
        assert!(!is_likely_sha("not-a-sha"));
    }

    #[test]
    fn match_full_and_short_shas() {
        assert!(sha_matches("123abcd", "123abcdef"));
        assert!(sha_matches("123abcdef", "123abcdef"));
        assert!(!sha_matches("123abce", "123abcdef"));
    }

    #[test]
    fn parse_version_refs() {
        assert_eq!(
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            }),
            parse_version("v1.2.3")
        );
        assert_eq!(
            Some(Version {
                major: 4,
                minor: 0,
                patch: 0
            }),
            parse_version("v4")
        );
        assert_eq!(
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            }),
            parse_version("1.2.3-beta")
        );
        assert_eq!(None, parse_version("main"));
    }
}
