//! A component whose dependency has no `#[injectable]` and isn't
//! `provide`d fails the whole graph build with a named error, rather than
//! panicking the first time something tries to use it.

use quench_http::di::{ContainerBuilder, DiError};
use quench_http::prelude::injectable;
use std::sync::Arc;

struct NotInjectable;

struct NeedsUnregistered {
    #[allow(dead_code)]
    dep: Arc<NotInjectable>,
}

#[injectable]
async fn provide_needs_unregistered(dep: Arc<NotInjectable>) -> NeedsUnregistered {
    NeedsUnregistered { dep }
}

#[tokio::test]
async fn missing_dependency_is_reported_by_name_not_a_panic() {
    let err = ContainerBuilder::new().build().await.unwrap_err();
    match err {
        DiError::MissingDependency {
            component,
            dependency,
        } => {
            assert!(
                component.contains("NeedsUnregistered"),
                "component was {component}"
            );
            assert!(
                dependency.contains("NotInjectable"),
                "dependency was {dependency}"
            );
        }
        other => panic!("expected MissingDependency, got {other:?}"),
    }
}
