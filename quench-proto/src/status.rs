//! HTTP status codes (RFC 9110 section 15).

use std::fmt;

/// A three-digit HTTP status code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(u16);

/// The number given to [`StatusCode::from_u16`] is outside `100..=999`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStatusCode;

impl fmt::Display for InvalidStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid status code")
    }
}

impl std::error::Error for InvalidStatusCode {}

impl StatusCode {
    pub const CONTINUE: Self = Self(100);
    pub const SWITCHING_PROTOCOLS: Self = Self(101);
    pub const PROCESSING: Self = Self(102);
    pub const EARLY_HINTS: Self = Self(103);
    pub const OK: Self = Self(200);
    pub const CREATED: Self = Self(201);
    pub const ACCEPTED: Self = Self(202);
    pub const NON_AUTHORITATIVE_INFORMATION: Self = Self(203);
    pub const NO_CONTENT: Self = Self(204);
    pub const RESET_CONTENT: Self = Self(205);
    pub const PARTIAL_CONTENT: Self = Self(206);
    pub const MULTI_STATUS: Self = Self(207);
    pub const ALREADY_REPORTED: Self = Self(208);
    pub const IM_USED: Self = Self(226);
    pub const MULTIPLE_CHOICES: Self = Self(300);
    pub const MOVED_PERMANENTLY: Self = Self(301);
    pub const FOUND: Self = Self(302);
    pub const SEE_OTHER: Self = Self(303);
    pub const NOT_MODIFIED: Self = Self(304);
    pub const USE_PROXY: Self = Self(305);
    pub const TEMPORARY_REDIRECT: Self = Self(307);
    pub const PERMANENT_REDIRECT: Self = Self(308);
    pub const BAD_REQUEST: Self = Self(400);
    pub const UNAUTHORIZED: Self = Self(401);
    pub const PAYMENT_REQUIRED: Self = Self(402);
    pub const FORBIDDEN: Self = Self(403);
    pub const NOT_FOUND: Self = Self(404);
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    pub const NOT_ACCEPTABLE: Self = Self(406);
    pub const PROXY_AUTHENTICATION_REQUIRED: Self = Self(407);
    pub const REQUEST_TIMEOUT: Self = Self(408);
    pub const CONFLICT: Self = Self(409);
    pub const GONE: Self = Self(410);
    pub const LENGTH_REQUIRED: Self = Self(411);
    pub const PRECONDITION_FAILED: Self = Self(412);
    pub const PAYLOAD_TOO_LARGE: Self = Self(413);
    pub const URI_TOO_LONG: Self = Self(414);
    pub const UNSUPPORTED_MEDIA_TYPE: Self = Self(415);
    pub const RANGE_NOT_SATISFIABLE: Self = Self(416);
    pub const EXPECTATION_FAILED: Self = Self(417);
    pub const IM_A_TEAPOT: Self = Self(418);
    pub const MISDIRECTED_REQUEST: Self = Self(421);
    pub const UNPROCESSABLE_ENTITY: Self = Self(422);
    pub const LOCKED: Self = Self(423);
    pub const FAILED_DEPENDENCY: Self = Self(424);
    pub const TOO_EARLY: Self = Self(425);
    pub const UPGRADE_REQUIRED: Self = Self(426);
    pub const PRECONDITION_REQUIRED: Self = Self(428);
    pub const TOO_MANY_REQUESTS: Self = Self(429);
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Self = Self(431);
    pub const UNAVAILABLE_FOR_LEGAL_REASONS: Self = Self(451);
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    pub const NOT_IMPLEMENTED: Self = Self(501);
    pub const BAD_GATEWAY: Self = Self(502);
    pub const SERVICE_UNAVAILABLE: Self = Self(503);
    pub const GATEWAY_TIMEOUT: Self = Self(504);
    pub const HTTP_VERSION_NOT_SUPPORTED: Self = Self(505);
    pub const VARIANT_ALSO_NEGOTIATES: Self = Self(506);
    pub const INSUFFICIENT_STORAGE: Self = Self(507);
    pub const LOOP_DETECTED: Self = Self(508);
    pub const NOT_EXTENDED: Self = Self(510);
    pub const NETWORK_AUTHENTICATION_REQUIRED: Self = Self(511);

    /// # Errors
    ///
    /// Returns [`InvalidStatusCode`] when `code` is not in `100..=999`.
    pub const fn from_u16(code: u16) -> Result<Self, InvalidStatusCode> {
        if code < 100 || code > 999 {
            Err(InvalidStatusCode)
        } else {
            Ok(Self(code))
        }
    }

    /// # Errors
    ///
    /// Returns [`InvalidStatusCode`] unless `src` is exactly three ASCII digits forming `100..=999`.
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidStatusCode> {
        let [a, b, c] = *src else {
            return Err(InvalidStatusCode);
        };
        if !(a.is_ascii_digit() && b.is_ascii_digit() && c.is_ascii_digit()) {
            return Err(InvalidStatusCode);
        }
        Self::from_u16(u16::from(a - b'0') * 100 + u16::from(b - b'0') * 10 + u16::from(c - b'0'))
    }

    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn is_informational(self) -> bool {
        self.0 >= 100 && self.0 < 200
    }

    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    #[must_use]
    pub const fn is_redirection(self) -> bool {
        self.0 >= 300 && self.0 < 400
    }

    #[must_use]
    pub const fn is_client_error(self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    #[must_use]
    pub const fn is_server_error(self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// The registered reason phrase, if there is one.
    #[must_use]
    pub const fn canonical_reason(self) -> Option<&'static str> {
        Some(match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            102 => "Processing",
            103 => "Early Hints",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            203 => "Non-Authoritative Information",
            204 => "No Content",
            205 => "Reset Content",
            206 => "Partial Content",
            207 => "Multi-Status",
            208 => "Already Reported",
            226 => "IM Used",
            300 => "Multiple Choices",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            305 => "Use Proxy",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            402 => "Payment Required",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            407 => "Proxy Authentication Required",
            408 => "Request Timeout",
            409 => "Conflict",
            410 => "Gone",
            411 => "Length Required",
            412 => "Precondition Failed",
            413 => "Content Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            416 => "Range Not Satisfiable",
            417 => "Expectation Failed",
            418 => "I'm a teapot",
            421 => "Misdirected Request",
            422 => "Unprocessable Content",
            423 => "Locked",
            424 => "Failed Dependency",
            425 => "Too Early",
            426 => "Upgrade Required",
            428 => "Precondition Required",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            451 => "Unavailable For Legal Reasons",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            505 => "HTTP Version Not Supported",
            506 => "Variant Also Negotiates",
            507 => "Insufficient Storage",
            508 => "Loop Detected",
            510 => "Not Extended",
            511 => "Network Authentication Required",
            _ => return None,
        })
    }

    /// Whether a response with this status never carries a body (1xx, 204, 304).
    #[must_use]
    pub const fn has_no_body(self) -> bool {
        self.is_informational() || self.0 == 204 || self.0 == 304
    }
}

impl Default for StatusCode {
    fn default() -> Self {
        Self::OK
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.0,
            self.canonical_reason().unwrap_or("<unknown status code>")
        )
    }
}

impl fmt::Debug for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl From<StatusCode> for u16 {
    fn from(code: StatusCode) -> Self {
        code.0
    }
}

impl TryFrom<u16> for StatusCode {
    type Error = InvalidStatusCode;

    fn try_from(code: u16) -> Result<Self, InvalidStatusCode> {
        Self::from_u16(code)
    }
}

impl PartialEq<u16> for StatusCode {
    fn eq(&self, other: &u16) -> bool {
        self.0 == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_and_classes() {
        assert_eq!(StatusCode::OK.as_u16(), 200);
        assert!(StatusCode::CONTINUE.is_informational());
        assert!(StatusCode::CREATED.is_success());
        assert!(StatusCode::FOUND.is_redirection());
        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(StatusCode::BAD_GATEWAY.is_server_error());
        assert!(!StatusCode::OK.is_client_error());
        assert_eq!(StatusCode::default(), StatusCode::OK);
        assert_eq!(StatusCode::OK, 200u16);
        assert_eq!(u16::from(StatusCode::IM_A_TEAPOT), 418);
    }

    #[test]
    fn parsing() {
        assert_eq!(StatusCode::from_u16(404).unwrap(), StatusCode::NOT_FOUND);
        assert_eq!(
            StatusCode::try_from(503).unwrap(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(StatusCode::from_u16(99).is_err() && StatusCode::from_u16(1000).is_err());
        assert_eq!(
            StatusCode::from_bytes(b"301").unwrap(),
            StatusCode::MOVED_PERMANENTLY
        );
        for bad in [&b"30"[..], b"3010", b"3a1", b"099", b""] {
            assert_eq!(
                StatusCode::from_bytes(bad),
                Err(InvalidStatusCode),
                "{bad:?}"
            );
        }
        assert!(InvalidStatusCode.to_string().contains("invalid"));
    }

    #[test]
    fn reasons_and_display() {
        assert_eq!(StatusCode::OK.canonical_reason(), Some("OK"));
        assert_eq!(StatusCode::OK.to_string(), "200 OK");
        assert_eq!(StatusCode::from_u16(599).unwrap().canonical_reason(), None);
        assert!(
            StatusCode::from_u16(599)
                .unwrap()
                .to_string()
                .contains("unknown")
        );
        assert_eq!(format!("{:?}", StatusCode::OK), "200");
    }

    #[test]
    fn every_registered_code_has_a_reason() {
        for code in 100..600u16 {
            let status = StatusCode::from_u16(code).unwrap();
            if let Some(reason) = status.canonical_reason() {
                assert!(!reason.is_empty());
            }
        }
        assert!(
            StatusCode::NO_CONTENT.has_no_body()
                && StatusCode::NOT_MODIFIED.has_no_body()
                && StatusCode::CONTINUE.has_no_body()
        );
        assert!(!StatusCode::OK.has_no_body());
    }
}
