//! The real machine: a few harmless commands any Unix has.

use super::support::tmp;
use quench_tool::error::Error;
use quench_tool::system::{Cmd, Host, System};
use std::path::Path;

#[quench_test::test]
fn run_returns_the_exit_code() {
    let cwd = Path::new(".");
    assert_eq!(
        Host.run(&Cmd::new("sh", &["-c", "exit 0"], cwd)).unwrap(),
        0
    );
    assert_eq!(
        Host.run(&Cmd::new("sh", &["-c", "exit 3"], cwd)).unwrap(),
        3
    );
    assert!(
        Host.run(&Cmd::new("no-such-program-anywhere", &[], cwd))
            .is_err()
    );
}

#[quench_test::test]
fn the_environment_and_directory_reach_the_command() {
    let dir = tmp("run-env");
    let cmd = Cmd::new(
        "sh",
        &["-c", "test \"$QUENCH_TEST_VAR\" = yes && test -f marker"],
        &dir,
    )
    .env("QUENCH_TEST_VAR", "yes");
    assert_eq!(Host.run(&cmd).unwrap(), 1, "no marker yet");
    std::fs::write(dir.join("marker"), "").unwrap();
    assert_eq!(Host.run(&cmd).unwrap(), 0);
    assert_eq!(
        cmd.line(),
        "sh -c test \"$QUENCH_TEST_VAR\" = yes && test -f marker"
    );
}

#[quench_test::test]
fn a_spawned_process_can_be_watched_and_stopped() {
    let mut running = Host
        .spawn(&Cmd::new("sleep", &["30"], Path::new(".")))
        .unwrap();
    assert_eq!(running.try_wait(), None);
    running.kill();
    assert_eq!(
        running.try_wait(),
        Some(-1),
        "killed by a signal: no exit code"
    );
    let mut quick = Host
        .spawn(&Cmd::new("sh", &["-c", "exit 7"], Path::new(".")))
        .unwrap();
    for _ in 0..200 {
        if quick.try_wait().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(quick.try_wait(), Some(7));
    assert!(
        Host.spawn(&Cmd::new("no-such-program-anywhere", &[], Path::new(".")))
            .is_err()
    );
}

#[quench_test::test]
fn capture_returns_the_output_of_a_command_that_succeeds() {
    assert!(
        Host.capture("sh", &["-c", "echo out; echo err >&2"])
            .unwrap()
            .contains("err")
    );
    assert!(
        Host.capture("sh", &["-c", "echo out"])
            .unwrap()
            .starts_with("out")
    );
    assert_eq!(Host.capture("sh", &["-c", "exit 1"]), None);
    assert_eq!(Host.capture("no-such-program-anywhere", &[]), None);
}

#[quench_test::test]
fn files_variables_and_the_path_are_looked_up() {
    let dir = tmp("lookup");
    std::fs::write(dir.join("b.txt"), "").unwrap();
    std::fs::write(dir.join("a.txt"), "").unwrap();
    assert!(Host.exists(&dir));
    assert!(!Host.exists(&dir.join("nope")));
    assert_eq!(Host.list_dir(&dir), ["a.txt", "b.txt"]);
    assert!(Host.list_dir(&dir.join("nope")).is_empty());
    assert!(Host.on_path("sh"));
    assert!(!Host.on_path("no-such-program-anywhere"));
    assert!(Host.var("PATH").is_some());
    assert_eq!(Host.var("QUENCH_TEST_SURELY_UNSET"), None);
}

#[quench_test::test]
fn errors_print_their_message() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    assert_eq!(Error::from(io).to_string(), "gone");
}

#[quench_test::test]
fn run_reports_through_the_exit_code() {
    assert_eq!(quench_tool::run(["--version"]), 0);
    assert_eq!(quench_tool::run(["no-such-command"]), 2);
}

#[quench_test::test]
fn a_free_port_is_found_and_a_listening_one_is_waited_for() {
    assert!(Host.free_port() > 0);
    // Port 0 is never connectable, so this is a timeout whatever else runs on the machine. (Probing
    // a port that was merely free a moment ago can connect a socket to itself and hold the port.)
    assert!(!Host.wait_port(0, 150));
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(Host.wait_port(port, 2000));
    drop(listener);
}
