---
title: API - logging, metrics, resilience
parent: quench-starter
grand_parent: Crates
nav_order: 4
---

## `logging` (`src/logging.rs`)

```rust
pub fn init();
```

Loads `.env` files, then `quench_log::init()`, then logs at debug level `Logging initialized with RUST_LOG={value or "info"}`. Details of the `.env` handling are in [Configuration](configuration.md). Also reachable as `quench_starter::init`.

## `metrics` (`src/metrics.rs`)

Counters for inter-service HTTP calls.

```rust
#[derive(Debug, Clone)]
pub struct RequestMetrics { /* Arc atomics and mutexes: clones share state */ }
impl RequestMetrics {
    #[must_use] pub fn new() -> Self;
    pub fn record_success(&self, latency_ms: u64);
    pub fn record_error(&self, error_msg: &str);
    #[must_use] pub fn snapshot(&self) -> MetricsSnapshot;
    pub fn reset(&self);
}
impl Default for RequestMetrics {}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_requests: u64, pub error_count: u64, pub success_count: u64,
    pub avg_latency_ms: u64, pub error_rate: f64,
    pub last_error: Option<String>, pub last_error_time: Option<Instant>,
}
impl MetricsSnapshot { #[must_use] pub fn to_prometheus(&self, metric_name: &str) -> String; }

#[derive(Debug)] pub struct TimedBlock { /* start: Instant */ }
impl TimedBlock {
    #[must_use] pub fn new() -> Self;
    #[must_use] pub fn elapsed_ms(&self) -> u64;
}
impl Default for TimedBlock {}
```

* `record_success` adds one request and `latency_ms` to the total latency. `record_error` adds one request and one error and stores the message and time (`Instant::now()`); its latency is not counted.
* `snapshot`: `total_requests` = request count (successes + errors); `success_count = total - errors` (saturating); `avg_latency_ms = total_latency / total_requests` (integer division; 0 if no requests; the divisor includes errors, whose latency is 0); `error_rate` = errors / total * 100 (0.0 if none).
* `reset` zeroes all counters and clears the last error.
* `to_prometheus(name)` renders four metrics, each with `# HELP`, `# TYPE` and a sample with an empty label set: `{name}_total_requests` (counter), `{name}_errors` (counter), `{name}_avg_latency_ms` (gauge), `{name}_error_rate` (gauge, two decimals). Example sample line: `upstream_total_requests {} 12`. `name` is not validated.
* `TimedBlock::elapsed_ms` saturates at `u64::MAX`.
* `record_error` and `reset` panic on a poisoned mutex; `snapshot` does not.

## `resilience` (`src/resilience.rs`)

```rust
#[derive(Clone, Debug)]
pub struct RetryConfig { pub max_attempts: u32, pub initial_delay_ms: u64, pub max_delay_ms: u64, pub backoff_multiplier: f64 }
impl Default for RetryConfig {}      // 3, 100, 5000, 2.0

pub async fn retry_with_backoff<F, Fut, T, E>(f: F, config: RetryConfig) -> Result<T, E>
    where F: FnMut() -> Fut, Fut: Future<Output = Result<T, E>>, E: std::fmt::Display;

#[must_use] pub fn format_error_message(error: &str, context: &str) -> String;

#[derive(Debug)] pub struct CircuitBreaker { /* thresholds, counters, mutexes */ }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum CircuitState { Closed, Open, HalfOpen }
impl CircuitBreaker {
    #[must_use] pub const fn new(failure_threshold: u32, success_threshold: u32, timeout_secs: u64) -> Self;
    pub fn call_succeeded(&self);
    pub fn call_failed(&self);
    pub fn is_available(&self) -> bool;
    pub fn get_state(&self) -> CircuitState;
}
impl Default for CircuitBreaker {}   // new(5, 3, 60)
```

### `retry_with_backoff`

Calls `f()` until it returns `Ok`, up to `max_attempts` attempts. After each failure that is not the last: logs at warn level `Attempt {n} failed: {err}. Retrying in {delay}ms...`, sleeps `delay` (starting at `initial_delay_ms`), then `delay = min(delay * backoff_multiplier, max_delay_ms)` (computed in `f64`, cast to `u64`). Returns the last error after the final attempt. With `max_attempts = 0` or `1`, `f` runs once.

### `format_error_message(error, context)`

Lower-cases `error` and picks the first match:

| Contains | Result |
|---|---|
| `timeout` or `time out` | `The {context} request timed out. The service may be busy or unreachable. Please try again.` |
| `connection` or `connect` | `Could not connect to {context}. Please check your internet connection and try again.` |
| `rate limit` or `429` | `{context} is rate limiting requests. Please wait a moment and try again.` |
| `unauthorized`, `401` or `api key` | `Authentication failed for {context}. The API key may be invalid or expired.` |
| `not found` or `404` | `{context} could not find what you're looking for. Try rephrasing your request.` |
| otherwise | `{context} encountered an error: {error}` |

### `CircuitBreaker`

* `call_failed`: increments the failure count, records the time; when failures `>= failure_threshold` and the state is not `Open`, the state becomes `Open` (warn log).
* `call_succeeded`: in `HalfOpen`, counts a success and, at `>= success_threshold`, goes to `Closed` and resets both counters (info log); in `Closed`, resets the failure count; in `Open`, nothing.
* `is_available`: `true` in `Closed` and `HalfOpen`. In `Open`, if at least `timeout_secs` seconds passed since the last failure, moves to `HalfOpen`, resets the success count and returns `true`; else `false`.
* `get_state`: current state.
* All methods panic on a poisoned mutex. The breaker does not run calls itself: the caller checks `is_available` and reports results. A failure in `HalfOpen` re-opens the circuit at once because the failure count is not reset on entering `HalfOpen`.
