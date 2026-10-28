//! Environment-variable helpers.
//!
//! Boolean parsing matches what the services already rely on: a variable is
//! `true` when it is set to anything except an empty string, `false`, `no` or
//! `0` (case-insensitive).

use std::ffi::{OsStr, OsString};

/// The variable's value, or `default` when it is unset or not valid UTF-8.
#[must_use]
pub fn get_or(key: impl AsRef<OsStr>, default: &str) -> String {
    var(key).unwrap_or_else(|| default.to_string())
}

/// The value of the first of `keys` that is set, or `default`.
#[must_use]
pub fn get_any<K: AsRef<OsStr>>(keys: &[K], default: &str) -> String {
    keys.iter()
        .find_map(var)
        .unwrap_or_else(|| default.to_string())
}

/// The variable parsed as a `T` (`get_parsed::<u64>("PORT", 8080)`), or `default` when it is unset, not
/// valid UTF-8 or does not parse.
#[must_use]
pub fn get_parsed<T: std::str::FromStr>(key: impl AsRef<OsStr>, default: T) -> T {
    var(key)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// Reads a boolean variable; `default` when it is unset.
#[must_use]
pub fn is_or(key: impl AsRef<OsStr>, default: bool) -> bool {
    var(key).map_or(default, |value| parse_bool(&value))
}

/// `true` unless empty, `false`, `no` or `0` (case-insensitive).
#[must_use]
pub fn parse_bool(value: &str) -> bool {
    let value = value.to_lowercase();
    !value.is_empty() && value != "false" && value != "no" && value != "0"
}

/// The value of a variable; the same as `std::env::var_os`, here so that readers go through this module.
#[must_use]
pub fn var_os(key: impl AsRef<OsStr>) -> Option<OsString> {
    std::env::var_os(key)
}

/// [`var_os`] as text; `None` when it is unset or not valid UTF-8.
#[must_use]
pub fn var(key: impl AsRef<OsStr>) -> Option<String> {
    var_os(key).and_then(|value| value.into_string().ok())
}

/// Sets a variable in the process's environment, where `std::env::var`, child processes and
/// code in other libraries all see it.
///
/// This is the one place in the workspace's libraries that changes the environment, and it is
/// `unsafe` inside because `std` says so: on Unix the environment is a global C array, and
/// changing it while another thread reads it (`getenv`, which libc itself calls for `TZ`,
/// `LANG`, the resolver and more) is undefined behaviour. Hence the contract, which the callers
/// keep:
///
/// - **Startup.** [`dotenv::load`](crate::dotenv::load) and the `.env` loader of `quench-starter` run
///   first thing in `main`, before the program starts a thread. Nothing else in the workspace
///   calls this outside tests.
/// - **Tests.** The test harness runs tests on threads, so a test that calls this may race with
///   a thread that reads the environment. The tests that do use variables of their own that
///   nothing else reads, and the libraries read the environment through [`var`] and the getters
///   only when asked, never in the background.
///
/// Anything else should pass its settings as values; `docs/quench-core.md` explains the contract.
pub fn set(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
    // SAFETY: the callers uphold the contract above: no other thread reads the environment while
    // this runs (startup code before any thread exists; tests on variables nothing else reads).
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Removes a variable from the process's environment, as [`set`] sets one. Same contract.
pub fn remove(key: impl AsRef<OsStr>) {
    // SAFETY: as for `set`.
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var(key);
    }
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
    fn a_change_is_the_real_environment_seen_by_std_children_and_the_getters() {
        // Each test owns its variable: tests run in parallel threads.
        set("QUENCH_CORE_T4", "x y");
        assert_eq!(var("QUENCH_CORE_T4").as_deref(), Some("x y"));
        assert_eq!(var_os("QUENCH_CORE_T4"), Some(OsString::from("x y")));
        assert_eq!(std::env::var("QUENCH_CORE_T4").as_deref(), Ok("x y"));
        // A child process inherits it.
        let child = std::process::Command::new("sh")
            .args(["-c", "printf %s \"$QUENCH_CORE_T4\""])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&child.stdout), "x y");
        // Setting again replaces; removing makes it unset for all of them.
        set("QUENCH_CORE_T4", "z");
        assert_eq!(std::env::var("QUENCH_CORE_T4").as_deref(), Ok("z"));
        remove("QUENCH_CORE_T4");
        assert_eq!(var("QUENCH_CORE_T4"), None);
        assert!(std::env::var_os("QUENCH_CORE_T4").is_none());
        let child = std::process::Command::new("sh")
            .args(["-c", "printf %s \"${QUENCH_CORE_T4-unset}\""])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&child.stdout), "unset");
        // Removing what was never set is fine.
        remove("QUENCH_CORE_T4");
    }

    #[quench_test::test]
    fn non_text_values_are_kept_for_var_os_and_hidden_from_var() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'a', 0xff, b'b']);
        set("QUENCH_CORE_T5", &raw);
        assert_eq!(var_os("QUENCH_CORE_T5"), Some(raw));
        assert_eq!(var("QUENCH_CORE_T5"), None, "not valid UTF-8");
        assert_eq!(get_or("QUENCH_CORE_T5", "dflt"), "dflt");
        remove("QUENCH_CORE_T5");
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
