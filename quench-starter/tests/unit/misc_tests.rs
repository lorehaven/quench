//! Health state, base paths, metrics output, and the resilience helpers' remaining branches.

use quench_starter::common::health::HealthState;
use quench_starter::common::routes::{normalize_base_path, with_base_path};
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::metrics::{RequestMetrics, TimedBlock};
use quench_starter::resilience::{
    CircuitBreaker, CircuitState, RetryConfig, format_error_message, retry_with_backoff,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[tokio::test]
async fn readiness_follows_the_init_future() {
    let state = HealthState::live();
    assert!(state.is_live() && !state.is_ready());
    let (release, wait) = tokio::sync::oneshot::channel::<()>();
    state.spawn_ready_when(async move {
        let _ = wait.await;
    });
    tokio::task::yield_now().await;
    assert!(!state.is_ready(), "still initialising");
    release.send(()).unwrap();
    for _ in 0..100 {
        if state.is_ready() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(state.is_ready());
    let dead = HealthState::default();
    assert!(!dead.is_live() && !dead.is_ready());
    dead.mark_ready();
    assert!(dead.is_ready());
}

#[test]
fn base_paths_normalise() {
    for (raw, expected) in [
        ("", "/"),
        ("/", "/"),
        ("  ", "/"),
        ("///", "/"),
        ("api", "/api"),
        ("/api/", "/api"),
        (" /api/v1// ", "/api/v1"),
    ] {
        assert_eq!(normalize_base_path(raw), expected, "{raw:?}");
    }
    // BASE_PATH is read once per process and is `/` in the test environment.
    assert_eq!(with_base_path("/x"), "/x");
    assert_eq!(with_base_path(""), "");
    assert_eq!(with_base_path("/"), "/");
}

#[test]
fn metrics_render_for_prometheus_and_reset() {
    let metrics = RequestMetrics::default();
    metrics.record_success(30);
    metrics.record_error("upstream said no");
    let snapshot = metrics.snapshot();
    assert_eq!(
        (
            snapshot.total_requests,
            snapshot.error_count,
            snapshot.success_count
        ),
        (2, 1, 1)
    );
    assert!((snapshot.error_rate - 50.0).abs() < f64::EPSILON);
    assert_eq!(snapshot.last_error.as_deref(), Some("upstream said no"));
    assert!(snapshot.last_error_time.is_some());
    let text = snapshot.to_prometheus("calls");
    assert!(
        text.contains("calls_total_requests {} 2")
            && text.contains("calls_errors {} 1")
            && text.contains("calls_avg_latency_ms {} 15")
            && text.contains("calls_error_rate {} 50.00"),
        "{text}"
    );
    metrics.reset();
    let empty = metrics.snapshot();
    assert_eq!((empty.total_requests, empty.avg_latency_ms), (0, 0));
    assert!(empty.last_error.is_none() && empty.error_rate.abs() < f64::EPSILON);
    let timer = TimedBlock::default();
    assert!(timer.elapsed_ms() < 1000);
    assert!(format!("{:?}", TimedBlock::new()).contains("TimedBlock"));
    assert!(format!("{metrics:?}").contains("RequestMetrics"));
}

#[test]
fn every_error_class_gets_its_own_message() {
    for (error, needle) in [
        ("operation timed out", "timed out"),
        ("Connection refused", "Could not connect"),
        ("HTTP 429 too many", "rate limiting"),
        ("rate limit exceeded", "rate limiting"),
        ("401 unauthorized", "Authentication failed"),
        ("invalid api key", "Authentication failed"),
        ("404 not found", "could not find"),
        ("something odd", "encountered an error: something odd"),
    ] {
        let message = format_error_message(error, "search");
        assert!(message.contains(needle), "{error} -> {message}");
    }
}

#[tokio::test]
async fn retries_give_up_after_the_configured_attempts() {
    let calls = Arc::new(AtomicU32::new(0));
    let seen = calls.clone();
    let config = RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 1,
        max_delay_ms: 2,
        backoff_multiplier: 10.0,
    };
    let result: Result<(), String> = retry_with_backoff(
        move || {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                Err("always".to_string())
            }
        },
        config.clone(),
    )
    .await;
    assert_eq!(result.unwrap_err(), "always");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(format!("{config:?}").contains("max_attempts"));
}

#[test]
fn the_circuit_breaker_recovers_through_half_open() {
    let breaker = CircuitBreaker::new(2, 2, 0);
    breaker.call_succeeded();
    breaker.call_failed();
    breaker.call_succeeded();
    assert_eq!(
        breaker.get_state(),
        CircuitState::Closed,
        "a success resets the failure count"
    );
    breaker.call_failed();
    breaker.call_failed();
    assert_eq!(breaker.get_state(), CircuitState::Open);
    breaker.call_succeeded();
    assert_eq!(
        breaker.get_state(),
        CircuitState::Open,
        "successes while open change nothing"
    );
    breaker.call_failed();
    assert_eq!(breaker.get_state(), CircuitState::Open);

    // A zero-second timeout lets the next availability check probe.
    assert!(breaker.is_available());
    assert_eq!(breaker.get_state(), CircuitState::HalfOpen);
    breaker.call_succeeded();
    assert_eq!(
        breaker.get_state(),
        CircuitState::HalfOpen,
        "one success is not enough"
    );
    breaker.call_succeeded();
    assert_eq!(breaker.get_state(), CircuitState::Closed);
    assert!(CircuitBreaker::default().is_available());

    let slow = CircuitBreaker::new(1, 1, 3600);
    slow.call_failed();
    assert!(!slow.is_available(), "still inside the timeout");
}

#[tokio::test]
async fn dependency_waits_and_the_gatehouse_probe_url() {
    quench_core::env::set("GATEHOUSE_URL", "https://gate.example.com/");
    assert_eq!(
        gatehouse_health_url(),
        "https://gate.example.com/health/ready"
    );
    quench_core::env::remove("GATEHOUSE_URL");
    assert_eq!(gatehouse_health_url(), "/health/ready");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut hits = 0;
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            hits += 1;
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            // The first probe fails, the second succeeds: the wait loop must retry.
            let status = if hits == 1 {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            let _ = tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await;
        }
    });
    let url = format!("http://{addr}/health/ready");
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        wait_for_services("test", vec![url.as_str()]),
    )
    .await
    .expect("the dependency became ready");
}
