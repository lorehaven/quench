//! `submit!` from outside the crate, the way `#[get]`/`#[injectable]` use it.

use quench_core::registry::Registry;
use quench_core::submit;

#[derive(Debug, PartialEq)]
struct Entry(&'static str);

static ENTRIES: Registry<Entry> = Registry::new();

submit!(ENTRIES, Entry, Entry("first"));
submit!(ENTRIES, Entry, Entry("second"));
submit!(ENTRIES, Entry, Entry("third"));

#[test]
fn constructors_register_before_main() {
    let names: Vec<&str> = ENTRIES.iter().map(|e| e.0).collect();
    assert_eq!(names.len(), 3);
    for expected in ["first", "second", "third"] {
        assert!(names.contains(&expected), "{names:?}");
    }
    assert_eq!(ENTRIES.len(), 3);
    assert!(!ENTRIES.is_empty());
}

#[test]
fn iteration_is_newest_first() {
    static LOCAL: Registry<Entry> = Registry::new();
    static A: Entry = Entry("a");
    static B: Entry = Entry("b");
    assert!(LOCAL.is_empty());
    LOCAL.register(&A);
    LOCAL.register(&B);
    assert_eq!(LOCAL.iter().map(|e| e.0).collect::<Vec<_>>(), ["b", "a"]);
    let fresh: Registry<Entry> = Registry::default();
    assert_eq!(fresh.len(), 0);
    assert!(format!("{LOCAL:?}").contains("Registry"));
}
