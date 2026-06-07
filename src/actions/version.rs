#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
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
