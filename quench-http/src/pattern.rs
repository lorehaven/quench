//! Route-pattern matching: literal text plus `{name}` (one non-empty path
//! segment), `{name:.+}` and `{name:.*}` (any run of characters, `/`
//! included - the docker-registry / sparse-index style catch-alls).
//!
//! Matching is anchored at both ends and backtracks, so a multi-segment
//! capture followed by a fixed suffix (`/{name:.+}/blobs/{digest}`) works
//! the way an anchored regex would. Anything fancier than those three
//! forms is rejected when the pattern is parsed, at registration time.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Literal(String),
    Segment(String),
    Rest { name: String, min: usize },
}

#[derive(Debug, Clone)]
pub struct Pattern {
    tokens: Vec<Token>,
}

impl Pattern {
    /// Panics on a malformed pattern - these are `&'static str` literals
    /// from `#[get]`/`.route(...)`, so it surfaces at startup.
    pub(crate) fn parse(pattern: &str) -> Self {
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut rest = pattern;

        while let Some(open) = rest.find('{') {
            literal.push_str(&rest[..open]);
            let Some(len) = rest[open..].find('}') else {
                panic!("route pattern `{pattern}`: unclosed `{{`");
            };
            let inner = &rest[open + 1..open + len];
            if !literal.is_empty() {
                tokens.push(Token::Literal(std::mem::take(&mut literal)));
            }
            tokens.push(match inner.split_once(':') {
                None => Token::Segment(inner.to_string()),
                Some((name, ".+")) => Token::Rest {
                    name: name.to_string(),
                    min: 1,
                },
                Some((name, ".*")) => Token::Rest {
                    name: name.to_string(),
                    min: 0,
                },
                Some((_, other)) => panic!(
                    "route pattern `{pattern}`: unsupported constraint `{other}` (only `.+` and `.*`)"
                ),
            });
            rest = &rest[open + len + 1..];
        }
        literal.push_str(rest);
        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }

        Self { tokens }
    }

    /// Captured `(name, value)` pairs when `path` matches the whole pattern.
    pub(crate) fn matches(&self, path: &str) -> Option<Vec<(String, String)>> {
        let mut captures = Vec::new();
        Self::walk(&self.tokens, path, &mut captures).then_some(captures)
    }

    fn walk(tokens: &[Token], path: &str, captures: &mut Vec<(String, String)>) -> bool {
        let Some((token, tail)) = tokens.split_first() else {
            return path.is_empty();
        };

        match token {
            Token::Literal(text) => path
                .strip_prefix(text.as_str())
                .is_some_and(|rest| Self::walk(tail, rest, captures)),
            Token::Segment(name) => {
                let limit = path.find('/').unwrap_or(path.len());
                Self::capture(name, 1, limit, tail, path, captures)
            }
            Token::Rest { name, min } => {
                Self::capture(name, *min, path.len(), tail, path, captures)
            }
        }
    }

    /// Greedy with backtracking: longest capture first, shrinking until the
    /// remaining tokens match.
    fn capture(
        name: &str,
        min: usize,
        limit: usize,
        tail: &[Token],
        path: &str,
        captures: &mut Vec<(String, String)>,
    ) -> bool {
        for end in (min..=limit).rev().filter(|&e| path.is_char_boundary(e)) {
            captures.push((name.to_string(), path[..end].to_string()));
            if Self::walk(tail, &path[end..], captures) {
                return true;
            }
            captures.pop();
        }
        false
    }
}

#[cfg(test)]
#[allow(clippy::literal_string_with_formatting_args)] // `{name}` is route-pattern syntax
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> Option<Vec<(String, String)>> {
        Pattern::parse(pattern).matches(path)
    }

    #[allow(clippy::unnecessary_wraps)] // compared against `Pattern::matches`, which returns an `Option`
    fn pairs(v: &[(&str, &str)]) -> Option<Vec<(String, String)>> {
        Some(
            v.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    #[test]
    fn literal_is_anchored_and_trailing_slash_sensitive() {
        assert!(m("/ui", "/ui").is_some());
        assert!(m("/ui", "/ui/").is_none());
        assert!(m("/ui/", "/ui").is_none());
        assert!(m("", "").is_some());
        assert!(m("/", "").is_none());
    }

    #[test]
    fn segment_captures_one_non_empty_segment() {
        assert_eq!(m("/u/{id}", "/u/42"), pairs(&[("id", "42")]));
        assert!(m("/u/{id}", "/u/").is_none());
        assert!(m("/u/{id}", "/u/4/2").is_none());
        assert_eq!(
            m("/u/{id}/x/{n}", "/u/1/x/2"),
            pairs(&[("id", "1"), ("n", "2")])
        );
    }

    #[test]
    fn segment_can_share_a_segment_with_literal_text() {
        assert_eq!(m("/f/{name}.json", "/f/a.json"), pairs(&[("name", "a")]));
    }

    #[test]
    fn rest_swallows_slashes_and_backtracks_for_a_suffix() {
        assert_eq!(
            m(
                "/{name:.+}/blobs/{digest}",
                "/library/ubuntu/blobs/sha256:ab"
            ),
            pairs(&[("name", "library/ubuntu"), ("digest", "sha256:ab")])
        );
        assert!(m("/{name:.+}/blobs/{digest}", "/blobs/x").is_none());
    }

    #[test]
    fn rest_min_distinguishes_plus_and_star() {
        assert_eq!(m("/a/{p:.*}", "/a/"), pairs(&[("p", "")]));
        assert!(m("/a/{p:.+}", "/a/").is_none());
        assert_eq!(m("/a/{p:.*}", "/a/b/c"), pairs(&[("p", "b/c")]));
    }

    #[test]
    #[should_panic(expected = "unsupported constraint")]
    fn unsupported_constraint_is_rejected() {
        Pattern::parse("/{id:[0-9]+}");
    }
}
