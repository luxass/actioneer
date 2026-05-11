use yamlpath::{Document, Feature, Route};

#[derive(Debug)]
pub struct ScalarRange<'a> {
    pub text: &'a str,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub trailing_comment: String,
}

pub fn parse(contents: &str) -> Result<Document, super::SyntaxError> {
    Document::new(contents.to_string()).map_err(|_| super::SyntaxError::InvalidYaml)
}

pub fn scalar_at_route<'a>(
    document: &'a Document,
    route: &Route<'_>,
) -> Result<Option<ScalarRange<'a>>, super::SyntaxError> {
    let Some(feature) = document
        .query_exact(route)
        .map_err(|_| super::SyntaxError::InvalidYaml)?
    else {
        return Ok(None);
    };

    let raw = document.extract(&feature);
    let text = clean_scalar(raw);
    let leading = raw.find(text).unwrap_or(0);
    let start_byte = feature.location.byte_span.0 + leading;
    let end_byte = start_byte + text.len();
    let line = feature.location.point_span.0 .0 + 1;
    let trailing_comment = trailing_comment(document, &feature);

    Ok(Some(ScalarRange {
        text,
        start_byte,
        end_byte,
        line,
        trailing_comment,
    }))
}

fn trailing_comment(document: &Document, feature: &Feature<'_>) -> String {
    document
        .feature_comments(feature)
        .into_iter()
        .filter(|comment| comment.location.point_span.0 .0 == feature.location.point_span.0 .0)
        .filter(|comment| comment.location.byte_span.0 >= feature.location.byte_span.1)
        .min_by_key(|comment| comment.location.byte_span.0)
        .map(|comment| {
            document
                .extract(&comment)
                .trim_start_matches('#')
                .trim_matches([' ', '\t'])
                .to_string()
        })
        .unwrap_or_default()
}

pub fn clean_scalar(value: &str) -> &str {
    let trimmed = value.trim_matches([' ', '\t']);
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use yamlpath::route;

    use super::*;

    #[test]
    fn scalar_at_route_removes_quotes_and_preserves_ref_span() {
        let source = "uses: \"actions/setup-node@v4\"\n";
        let document = parse(source).unwrap();
        let value = scalar_at_route(&document, &route!("uses"))
            .unwrap()
            .unwrap();
        assert_eq!("actions/setup-node@v4", value.text);
        assert_eq!(
            "actions/setup-node@v4",
            &source[value.start_byte..value.end_byte]
        );
    }

    #[test]
    fn scalar_at_route_finds_trailing_comment() {
        let source = "uses: \"actions/setup-node@v4#literal\" # v4.2.0\n";
        let document = parse(source).unwrap();
        let value = scalar_at_route(&document, &route!("uses"))
            .unwrap()
            .unwrap();
        assert_eq!("v4.2.0", value.trailing_comment);
    }
}
