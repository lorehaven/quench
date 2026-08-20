//! Unit tests for `common/wait.rs`.

use actix_web::{App, HttpResponse, HttpServer, web};
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use std::time::Duration;

#[test]
fn gatehouse_health_url_appends_the_ready_path_and_strips_a_trailing_slash() {
    unsafe {
        std::env::set_var("GATEHOUSE_URL", "https://gatehouse.internal/");
    }
    assert_eq!(
        gatehouse_health_url(),
        "https://gatehouse.internal/health/ready"
    );
    unsafe {
        std::env::remove_var("GATEHOUSE_URL");
    }
}

#[test]
fn gatehouse_health_url_defaults_to_a_bare_ready_path_when_unset() {
    unsafe {
        std::env::remove_var("GATEHOUSE_URL");
    }
    assert_eq!(gatehouse_health_url(), "/health/ready");
}

#[actix_web::test]
async fn wait_for_services_returns_once_the_dependency_answers_successfully() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    let addr = listener.local_addr().expect("a local address");

    let server = HttpServer::new(|| {
        App::new().route(
            "/health/ready",
            web::get().to(|| async { HttpResponse::Ok().finish() }),
        )
    })
    .listen(listener)
    .expect("listen on the bound socket")
    .run();
    let handle = server.handle();
    let server_task = actix_web::rt::spawn(server);

    let url = format!("http://{addr}/health/ready");
    tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_services("test-service", vec![url.as_str()]),
    )
    .await
    .expect("wait_for_services should resolve once the dependency is healthy");

    handle.stop(true).await;
    let _ = server_task.await;
}
