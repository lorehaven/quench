//! Environment-variable helpers.
//!
//! Boolean parsing matches what the services already rely on: a variable is
//! `true` when it is set to anything except an empty string, `false`, `no` or
//! `0` (case-insensitive).

use std::ffi::OsStr;

/// The variable's value, or `default` when it is unset or not valid UTF-8.
#[must_use]
pub fn get_or(key: impl AsRef<OsStr>, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// The value of the first of `keys` that is set, or `default`.
#[must_use]
pub fn get_any<K: AsRef<OsStr>>(keys: &[K], default: &str) -> String {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .unwrap_or_else(|| default.to_string())
}

/// The variable parsed as a `T` (`get_parsed::<u64>("PORT", 8080)`), or `default` when it is unset, not
/// valid UTF-8 or does not parse.
#[must_use]
pub fn get_parsed<T: std::str::FromStr>(key: impl AsRef<OsStr>, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// Reads a boolean variable; `default` when it is unset.
#[must_use]
pub fn is_or(key: impl AsRef<OsStr>, default: bool) -> bool {
    std::env::var(key).map_or(default, |value| parse_bool(&value))
}

/// `true` unless empty, `false`, `no` or `0` (case-insensitive).
#[must_use]
pub fn parse_bool(value: &str) -> bool {
    let value = value.to_lowercase();
    !value.is_empty() && value != "false" && value != "no" && value != "0"
}

/// Sets a variable for this process.
///
/// Meant for tests and for startup code that runs before any other thread
/// does: changing the environment while another thread reads it through C
/// (`getenv`) is undefined behaviour, which is why `std` marks this unsafe.
#[allow(unsafe_code)]
pub fn set(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
    // SAFETY: documented above - callers uphold the single-threaded contract.
    unsafe { std::env::set_var(key, value) };
}

/// Removes a variable. Same contract as [`set`].
#[allow(unsafe_code)]
pub fn remove(key: impl AsRef<OsStr>) {
    // SAFETY: as for `set`.
    unsafe { std::env::remove_var(key) };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test owns distinctly named variables: tests run in parallel threads.

    #[quench_test::test]
    fn get_parsed_falls_back_on_anything_unusable() {
        assert_eq!(get_parsed::<u64>("QUENCH_TEST_PARSED_UNSET", 7), 7);
        set("QUENCH_TEST_PARSED_OK", " 42 ");
        assert_eq!(get_parsed::<u64>("QUENCH_TEST_PARSED_OK", 7), 42);
        set("QUENCH_TEST_PARSED_BAD", "forty");
        assert_eq!(get_parsed::<u64>("QUENCH_TEST_PARSED_BAD", 7), 7);
        assert!((get_parsed::<f64>("QUENCH_TEST_PARSED_OK", 0.5) - 42.0).abs() < f64::EPSILON);
    }

    #[quench_test::test]
    fn get_or_falls_back() {
        remove("QUENCH_CORE_T1");
        assert_eq!(get_or("QUENCH_CORE_T1", "dflt"), "dflt");
        set("QUENCH_CORE_T1", "value");
        assert_eq!(get_or("QUENCH_CORE_T1", "dflt"), "value");
        remove("QUENCH_CORE_T1");
        assert_eq!(get_or("QUENCH_CORE_T1", "dflt"), "dflt");
    }

    #[quench_test::test]
    fn get_any_takes_the_first_set_key() {
        remove("QUENCH_CORE_T2A");
        remove("QUENCH_CORE_T2B");
        assert_eq!(
            get_any(&["QUENCH_CORE_T2A", "QUENCH_CORE_T2B"], "none"),
            "none"
        );
        set("QUENCH_CORE_T2B", "b");
        assert_eq!(
            get_any(&["QUENCH_CORE_T2A", "QUENCH_CORE_T2B"], "none"),
            "b"
        );
        set("QUENCH_CORE_T2A", "a");
        assert_eq!(
            get_any(&["QUENCH_CORE_T2A", "QUENCH_CORE_T2B"], "none"),
            "a"
        );
        remove("QUENCH_CORE_T2A");
        remove("QUENCH_CORE_T2B");
    }

    #[quench_test::test]
    fn bool_parsing() {
        for truthy in ["true", "TRUE", "1", "yes", "on", "anything", " "] {
            assert!(parse_bool(truthy), "{truthy:?}");
        }
        for falsy in ["", "false", "False", "NO", "no", "0"] {
            assert!(!parse_bool(falsy), "{falsy:?}");
        }
    }

    #[quench_test::test]
    fn is_or_uses_the_default_only_when_unset() {
        remove("QUENCH_CORE_T3");
        assert!(is_or("QUENCH_CORE_T3", true));
        assert!(!is_or("QUENCH_CORE_T3", false));
        set("QUENCH_CORE_T3", "no");
        assert!(!is_or("QUENCH_CORE_T3", true));
        set("QUENCH_CORE_T3", "yes");
        assert!(is_or("QUENCH_CORE_T3", false));
        remove("QUENCH_CORE_T3");
    }
}
