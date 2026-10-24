# Quench Test

`quench-test` (crate `quench_test`) is a test harness without dependencies beyond Quench's own crates. It runs `#[quench_test::test]` functions with the command line and output of the standard harness (`cargo test` works unchanged), and writes the results of every run as JSON and as a self-contained HTML page. It also carries the helpers tests of services need: a command runner, predicates and a mock HTTP server. It replaces `assert_cmd`, `predicates` and `wiremock`.

| Module | Replaces | |
| --- | --- | --- |
| `quench_test::test` (attribute) | `#[test]`, `#[tokio::test]` | sync and `async` tests, `flavor = "multi_thread", worker_threads = N`, `Result` returns, `#[ignore]`, `#[should_panic(expected = ..)]` |
| `cmd` | `assert_cmd` | `Command::cargo_bin`, `Assert::success`/`failure`/`code`/`stdout`/`stderr` |
| `predicate` | `predicates` | `contains`, `starts_with`, `ends_with`, `is_empty`, `is_match`, `eq`, `and`/`or`/`not` |
| `mock` | `wiremock` | `MockServer`, `Mock::given(..).respond_with(..)`, matchers, expectations |
| `report` | | the result model, `to_json`, `html`, `write` |

## Using the harness

A test target switches the standard harness off and installs ours:

```toml
[[test]]
name = "api"
path = "tests/api.rs"
harness = false
```

```rust
use quench_test::test;

quench_test::main!();

#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn serves() -> Result<(), String> {
    Ok(())
}
```

The binary understands `cargo test`'s arguments: filters, `--exact`, `--skip`, `--ignored`, `--include-ignored`, `--list`, `--test-threads` and `--quiet` (`--nocapture`, `--show-output`, `--color` and `--format` are accepted and ignored), with the same output (`running N tests`, `test x ... ok`, the failures section, `test result: ok. N passed; M failed; ...`). A failing run exits with 101.

## Reports

At the end of every run `<package>-<binary>.json` and `.html` are written to `target/<profile>/quench-test/`, and `index.html` and `summary.json` there are rebuilt from every report in the directory, so one `cargo test` leaves a single page covering all test binaries. The page is one file with no external resources, in light and dark, with a filter for the status of each case and the steps of BDD scenarios.

- `QUENCH_TEST_REPORT_DIR=path` or `--report-dir path` writes elsewhere; `QUENCH_TEST_REPORT=off` writes nothing.
- The JSON has `title`, `started`, `duration_ms`, a `summary` (`total`, `passed`, `failed`, `skipped`) and `suites`, each with its `cases` (`name`, `status`, `duration_ms`, `message`, `tags`, `location`, `steps`). `report::Report::from_json` reads it back.

## Commands

```rust
use quench_test::cmd::Command;
use quench_test::predicate::{self, Predicate};

Command::cargo_bin("anvil")?
    .args(["lint", "--quiet"])
    .env("NO_COLOR", "1")
    .current_dir(&workspace)
    .write_stdin("input")
    .timeout(std::time::Duration::from_secs(60))
    .assert()
    .success()
    .stdout(predicate::str::contains("ok").and(predicate::str::is_empty().not()));
```

`output()` returns the process output instead; `stdout`/`stderr` take a predicate, a `&str`/`String` (exact text) or `Exactly`.

## Mock server

```rust
use quench_test::mock::{Mock, MockServer, ResponseTemplate, matchers::{method, path, header}};

let server = MockServer::start().await;
Mock::given(method("GET"))
    .and(path("/v1/models"))
    .and(header("authorization", "Bearer t"))
    .respond_with(ResponseTemplate::new(200).set_body_json(&models).insert_header("x-a", "b"))
    .expect(1)
    .mount(&server)
    .await;
let url = server.uri();
```

An HTTP/1.1 server on a random local port. Rules are tried by priority, then in the order mounted; a request nobody claims gets a 404 and is still recorded (`received_requests`). Matchers: `method`, `path`, `path_regex`, `header`, `header_regex`, `header_exists`, `query_param`, `body_string`, `body_string_contains`, `body_bytes`, `body_json`, `body_json_string`, `basic_auth`, `any`. `expect(n)` (or a range), `up_to_n_times`, `with_priority` and `named` follow `wiremock`; expectations are checked when the server drops (a failure panics) or on `verify().await`. Responses: `set_body_string`/`json`/`bytes`/`raw`, `insert_header`, `set_delay`.
