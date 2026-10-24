//! Conditions on requests for [`Mock::given`](super::Mock::given) and [`and`](super::MockBuilder::and).

use super::{Match, Request};
use quench_regex::Regex;
use quench_serde::Value;

macro_rules! matcher {
    ($(#[$doc:meta])* $name:ident($($field:ident: $ty:ty),*), |$self_:ident, $request:ident| $body:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $name {
            $($field: $ty),*
        }

        impl Match for $name {
            fn matches(&$self_, $request: &Request) -> bool {
                $body
            }
        }
    };
}

matcher!(
    /// The HTTP method, compared case-insensitively.
    MethodMatcher(method: String),
    |self, request| request.method.eq_ignore_ascii_case(&self.method)
);
matcher!(
    /// The exact path.
    PathMatcher(path: String),
    |self, request| request.path == self.path
);
matcher!(
    /// A path matching the pattern anywhere (anchor it with `^` and `$`).
    PathRegexMatcher(regex: Regex),
    |self, request| self.regex.is_match(&request.path)
);
matcher!(
    /// A header with exactly this value.
    HeaderMatcher(name: String, value: String),
    |self, request| request.header(&self.name).is_some_and(|v| v == self.value)
);
matcher!(
    /// A header whose value matches the pattern.
    HeaderRegexMatcher(name: String, regex: Regex),
    |self, request| request.header(&self.name).is_some_and(|v| self.regex.is_match(v))
);
matcher!(
    /// A header that is present, whatever its value.
    HeaderExistsMatcher(name: String),
    |self, request| request.header(&self.name).is_some()
);
matcher!(
    /// A query parameter with exactly this decoded value.
    QueryParamMatcher(name: String, value: String),
    |self, request| request.query_param(&self.name).is_some_and(|v| v == self.value)
);
matcher!(
    /// The body, as text, equal to this.
    BodyStringMatcher(text: String),
    |self, request| request.body_string() == self.text
);
matcher!(
    /// The body, as text, containing this.
    BodyContainsMatcher(text: String),
    |self, request| request.body_string().contains(&self.text)
);
matcher!(
    /// The body equal to these bytes.
    BodyBytesMatcher(bytes: Vec<u8>),
    |self, request| request.body == self.bytes
);
matcher!(
    /// The body parsed as JSON equal to this value.
    BodyJsonMatcher(value: Value),
    |self, request| request.body_json().is_some_and(|v| v == self.value)
);
matcher!(
    /// HTTP basic authentication with this user and password.
    BasicAuthMatcher(expected: String),
    |self, request| request.header("authorization").is_some_and(|v| v == self.expected)
);

/// Every request.
#[derive(Debug, Clone, Copy)]
pub struct AnyMatcher;

impl Match for AnyMatcher {
    fn matches(&self, _: &Request) -> bool {
        true
    }
}

#[must_use]
pub fn method(method: impl AsRef<str>) -> MethodMatcher {
    let method = method.as_ref();
    MethodMatcher {
        method: method.to_string(),
    }
}

#[must_use]
pub fn path(path: impl AsRef<str>) -> PathMatcher {
    let path = path.as_ref();
    PathMatcher {
        path: path.to_string(),
    }
}

/// # Panics
///
/// When `pattern` is not a valid regular expression.
#[must_use]
pub fn path_regex(pattern: impl AsRef<str>) -> PathRegexMatcher {
    let pattern = pattern.as_ref();
    PathRegexMatcher {
        regex: Regex::new(pattern).unwrap_or_else(|e| panic!("bad path pattern {pattern:?}: {e}")),
    }
}

#[must_use]
pub fn header(name: impl AsRef<str>, value: impl AsRef<str>) -> HeaderMatcher {
    let name = name.as_ref();
    let value = value.as_ref();
    HeaderMatcher {
        name: name.to_string(),
        value: value.to_string(),
    }
}

/// # Panics
///
/// When `pattern` is not a valid regular expression.
#[must_use]
pub fn header_regex(name: impl AsRef<str>, pattern: impl AsRef<str>) -> HeaderRegexMatcher {
    let name = name.as_ref();
    let pattern = pattern.as_ref();
    HeaderRegexMatcher {
        name: name.to_string(),
        regex: Regex::new(pattern)
            .unwrap_or_else(|e| panic!("bad header pattern {pattern:?}: {e}")),
    }
}

#[must_use]
pub fn header_exists(name: impl AsRef<str>) -> HeaderExistsMatcher {
    let name = name.as_ref();
    HeaderExistsMatcher {
        name: name.to_string(),
    }
}

#[must_use]
pub fn query_param(name: impl AsRef<str>, value: impl AsRef<str>) -> QueryParamMatcher {
    let name = name.as_ref();
    let value = value.as_ref();
    QueryParamMatcher {
        name: name.to_string(),
        value: value.to_string(),
    }
}

#[must_use]
pub fn body_string(text: impl AsRef<str>) -> BodyStringMatcher {
    let text = text.as_ref();
    BodyStringMatcher {
        text: text.to_string(),
    }
}

#[must_use]
pub fn body_string_contains(text: impl AsRef<str>) -> BodyContainsMatcher {
    let text = text.as_ref();
    BodyContainsMatcher {
        text: text.to_string(),
    }
}

#[must_use]
pub fn body_bytes(bytes: impl Into<Vec<u8>>) -> BodyBytesMatcher {
    BodyBytesMatcher {
        bytes: bytes.into(),
    }
}

/// The body must parse as JSON equal to `value`.
#[must_use]
pub fn body_json(value: impl quench_serde::Serialize) -> BodyJsonMatcher {
    BodyJsonMatcher {
        value: quench_serde::to_value(&value),
    }
}

/// The body must parse as JSON equal to the JSON text `json`.
///
/// # Panics
///
/// When `json` is not valid JSON.
#[must_use]
pub fn body_json_string(json: impl AsRef<str>) -> BodyJsonMatcher {
    let json = json.as_ref();
    BodyJsonMatcher {
        value: quench_serde::json::from_str::<Value>(json)
            .unwrap_or_else(|e| panic!("bad JSON in matcher: {e}")),
    }
}

#[must_use]
pub fn basic_auth(user: impl AsRef<str>, password: impl AsRef<str>) -> BasicAuthMatcher {
    let user = user.as_ref();
    let password = password.as_ref();
    BasicAuthMatcher {
        expected: format!(
            "Basic {}",
            quench_core::base64::STANDARD.encode(format!("{user}:{password}"))
        ),
    }
}

#[must_use]
pub const fn any() -> AnyMatcher {
    AnyMatcher
}
