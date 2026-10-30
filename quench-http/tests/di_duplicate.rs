//! Two `#[injectable]` functions for one type are reported by name, not as a nameless cycle.
#![allow(clippy::unused_async)]
// `#[injectable]` constructors are async by contract

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_http::di::{ContainerBuilder, DiError};
use quench_http::prelude::injectable;

struct Pool;

#[injectable]
async fn first_pool() -> Pool {
    Pool
}

#[injectable]
async fn second_pool() -> Pool {
    Pool
}

#[quench_test::test]
async fn two_factories_for_one_type_are_a_duplicate_not_a_cycle() {
    match ContainerBuilder::new().build().await.unwrap_err() {
        DiError::Duplicate(name) => assert!(name.contains("Pool"), "{name}"),
        other => panic!("expected Duplicate, got {other:?}"),
    }
}

quench_test::main!();
