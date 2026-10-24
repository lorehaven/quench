//! `#[async_trait]` as the rest of quench uses it: object-safe async traits.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use quench_core::async_trait;
use quench_core::future::block_on;

#[async_trait]
trait Store: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn put(&self, key: &str, value: String);

    /// A default body that calls the required methods.
    async fn get_or(&self, key: &str, fallback: &str) -> String {
        self.get(key).await.unwrap_or_else(|| fallback.to_string())
    }
}

#[derive(Default)]
struct Memory(std::sync::Mutex<Vec<(String, String)>>);

#[async_trait]
impl Store for Memory {
    async fn get(&self, key: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    async fn put(&self, key: &str, value: String) {
        self.0.lock().unwrap().push((key.to_string(), value));
    }
}

#[quench_test::test]
fn works_as_a_trait_object() {
    let store: Arc<dyn Store> = Arc::new(Memory::default());
    block_on(async {
        assert_eq!(store.get("a").await, None);
        store.put("a", "1".into()).await;
        assert_eq!(store.get("a").await.as_deref(), Some("1"));
        assert_eq!(store.get_or("a", "x").await, "1");
        assert_eq!(store.get_or("zzz", "x").await, "x");
    });
}

#[quench_test::test]
fn futures_are_send() {
    let store: Arc<dyn Store> = Arc::new(Memory::default());
    let worker = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || block_on(async move { store.put("k", "v".into()).await }))
    };
    worker.join().unwrap();
    assert_eq!(block_on(store.get("k")).as_deref(), Some("v"));
}

#[async_trait]
trait Mutating {
    async fn bump(&mut self, by: u32) -> u32;
    async fn consume(self) -> String
    where
        Self: Sized;
}

struct Counter(u32);

#[async_trait]
impl Mutating for Counter {
    async fn bump(&mut self, by: u32) -> u32 {
        self.0 += by;
        self.0
    }

    async fn consume(self) -> String {
        format!("final={}", self.0)
    }
}

#[quench_test::test]
fn mutable_and_by_value_receivers() {
    block_on(async {
        let mut counter = Counter(1);
        assert_eq!(counter.bump(2).await, 3);
        assert_eq!(counter.bump(4).await, 7);
        assert_eq!(counter.consume().await, "final=7");
    });
}

#[async_trait]
trait Combine {
    async fn join_all<'a>(
        &self,
        parts: &'a [&'a str],
        sep: Option<&str>,
        tail: &mut Vec<String>,
    ) -> String;
    async fn generic<T: ToString + Send + Sync>(&self, value: T) -> String;
    async fn build() -> Self
    where
        Self: Sized;
}

struct Joiner;

#[async_trait]
impl Combine for Joiner {
    async fn join_all<'a>(
        &self,
        parts: &'a [&'a str],
        sep: Option<&str>,
        tail: &mut Vec<String>,
    ) -> String {
        tail.push("seen".to_string());
        parts.join(sep.unwrap_or(","))
    }

    async fn generic<T: ToString + Send + Sync>(&self, value: T) -> String {
        format!("<{}>", value.to_string())
    }

    async fn build() -> Self {
        Self
    }
}

#[quench_test::test]
fn many_borrows_generics_and_static_constructors() {
    block_on(async {
        let joiner = Joiner::build().await;
        let mut tail = Vec::new();
        assert_eq!(
            joiner.join_all(&["a", "b"], Some("-"), &mut tail).await,
            "a-b"
        );
        assert_eq!(joiner.join_all(&["a", "b"], None, &mut tail).await, "a,b");
        assert_eq!(tail, ["seen", "seen"]);
        assert_eq!(joiner.generic(42).await, "<42>");
        assert_eq!(joiner.generic("s").await, "<s>");
    });
}

#[async_trait(?Send)]
trait Local {
    async fn push(&self, value: u8);
}

struct Shared(Rc<RefCell<Vec<u8>>>);

#[async_trait(?Send)]
impl Local for Shared {
    async fn push(&self, value: u8) {
        self.0.borrow_mut().push(value);
    }
}

#[quench_test::test]
fn question_send_allows_non_send_state() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let local: Box<dyn Local> = Box::new(Shared(Rc::clone(&log)));
    block_on(async {
        local.push(1).await;
        local.push(2).await;
    });
    assert_eq!(*log.borrow(), [1, 2]);
}

struct Wrapper<T>(T);

#[async_trait]
impl<T: Store> Store for Wrapper<T> {
    async fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).await.map(|v| format!("[{v}]"))
    }

    async fn put(&self, key: &str, value: String) {
        self.0.put(key, value).await;
    }
}

#[quench_test::test]
fn generic_impls_compose() {
    block_on(async {
        let wrapped = Wrapper(Memory::default());
        wrapped.put("k", "v".into()).await;
        assert_eq!(wrapped.get("k").await.as_deref(), Some("[v]"));
        assert_eq!(wrapped.get_or("none", "d").await, "d");
    });
}

#[async_trait]
trait Fallible {
    async fn run(&self, ok: bool) -> Result<u8, String>;
}

struct Task;

#[async_trait]
impl Fallible for Task {
    async fn run(&self, ok: bool) -> Result<u8, String> {
        let value: u8 = if ok {
            "7".parse().map_err(|e| format!("{e}"))?
        } else {
            "x".parse().map_err(|e| format!("{e}"))?
        };
        Ok(value)
    }
}

#[quench_test::test]
fn question_mark_and_early_return_work() {
    block_on(async {
        assert_eq!(Task.run(true).await, Ok(7));
        assert!(Task.run(false).await.is_err());
    });
}

quench_test::main!();
