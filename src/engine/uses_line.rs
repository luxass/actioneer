//! Parse and rewrite `uses:` lines in workflow YAML source.

/// Quoting style used by a parsed `uses:` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    /// The `uses:` value is unquoted.
    None,
    /// The `uses:` value is wrapped in double quotes.
    Double,
    /// The `uses:` value is wrapped in single quotes.
    Single,
}

/// The `uses:` portion of one workflow source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsesLine {
    /// Everything through the whitespace after `uses:` (indent, `- `, key).
    pub head: String,
    /// Unquoted `owner/repo@ref`.
    pub value: String,
    /// Quoting style preserved when rebuilding the source line.
    pub quote: QuoteStyle,
    /// Trailing `# comment` text, without the `#`.
    pub comment: Option<String>,
}

/// Split a source line that contains a `uses:` key.
pub fn split(line: &str) -> Option<UsesLine> {
    let value_start = uses_value_start(line)?;
    let head = line[..value_start].to_string();
    let (quote, value, comment) = parse_uses_value(line[value_start..].trim_start())?;
    Some(UsesLine {
        head,
        value,
        quote,
        comment,
    })
}

/// Rebuild a full source line with a new action value and comment.
pub fn join(line: &UsesLine, value: &str, comment: Option<&str>) -> String {
    format!(
        "{}{}",
        line.head,
        format_uses_value(value, line.quote, comment)
    )
}

/// Byte index in `line` where the `uses:` value begins.
pub fn uses_value_start(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let dash_len = trimmed.len() - after_dash.len();
    let uses_rel = after_dash.find("uses:")?;
    let after_key = &after_dash[uses_rel + "uses:".len()..];
    let ws = after_key.len() - after_key.trim_start().len();
    Some(indent + dash_len + uses_rel + "uses:".len() + ws)
}

/// Parse the value and comment from the text after `uses:` (already past leading ws).
pub(crate) fn parse_value_and_comment(rest: &str) -> (String, Option<String>) {
    let trimmed = rest.trim();

    if let Some(inner) = trimmed.strip_prefix('"') {
        if let Some(close) = inner.find('"') {
            let value = &inner[..close];
            let remainder = &inner[close + 1..];
            return (value.to_string(), comment_from_remainder(remainder));
        }
    } else if let Some(inner) = trimmed.strip_prefix('\'')
        && let Some(close) = inner.find('\'')
    {
        let value = &inner[..close];
        let remainder = &inner[close + 1..];
        return (value.to_string(), comment_from_remainder(remainder));
    }

    if let Some(hash) = trimmed.find('#') {
        let value = trimmed[..hash].trim_end();
        let comment_text = trimmed[hash + 1..].trim();
        return (
            value.to_string(),
            if comment_text.is_empty() {
                None
            } else {
                Some(comment_text.to_string())
            },
        );
    }

    (trimmed.to_string(), None)
}

fn parse_uses_value(rest: &str) -> Option<(QuoteStyle, String, Option<String>)> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return None;
    }

    let quote = if trimmed.starts_with('"') {
        QuoteStyle::Double
    } else if trimmed.starts_with('\'') {
        QuoteStyle::Single
    } else {
        QuoteStyle::None
    };

    let (value, comment) = parse_value_and_comment(rest);
    if value.is_empty() {
        return None;
    }
    Some((quote, value, comment))
}

fn format_uses_value(value: &str, quote: QuoteStyle, comment: Option<&str>) -> String {
    let quoted = match quote {
        QuoteStyle::None => value.to_string(),
        QuoteStyle::Double => format!("\"{value}\""),
        QuoteStyle::Single => format!("'{value}'"),
    };
    match comment.filter(|c| !c.is_empty()) {
        Some(text) => format!("{quoted} # {text}"),
        None => quoted,
    }
}

fn comment_from_remainder(remainder: &str) -> Option<String> {
    let hash = remainder.find('#')?;
    let text = remainder[hash + 1..].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_unquoted_with_comment() {
        let line = "        uses: actions/checkout@v4 # v4.2.0";
        let parts = split(line).unwrap();
        assert_eq!(parts.head, "        uses: ");
        assert_eq!(parts.value, "actions/checkout@v4");
        assert_eq!(parts.comment.as_deref(), Some("v4.2.0"));
        assert_eq!(parts.quote, QuoteStyle::None);
    }

    #[test]
    fn join_preserves_indent_and_comment() {
        let line = split("      - uses: actions/checkout@v4 # v4").unwrap();
        let rebuilt = join(&line, "actions/checkout@v4.2.0", Some("v4.2.0"));
        assert_eq!(rebuilt, "      - uses: actions/checkout@v4.2.0 # v4.2.0");
    }

    #[test]
    fn split_sha_with_tag_comment() {
        let line = "        uses: actions/checkout@aaaa # v6.0.3";
        let parts = split(line).unwrap();
        assert_eq!(parts.value, "actions/checkout@aaaa");
        assert_eq!(parts.comment.as_deref(), Some("v6.0.3"));
    }
}
