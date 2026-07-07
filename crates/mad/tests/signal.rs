//! Process-level signal tests: spawn the real `staged_shutdown_demo` binary and
//! send it real SIGTERM / SIGKILL, asserting the OS signal path, ordered
//! graceful drain, honored delays, and exit status. Unix only.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DEMO: &str = env!("CARGO_BIN_EXE_staged_shutdown_demo");
const DRAIN_MS: u64 = 200;

/// Spawn the demo with a piped stdout and wait until it prints `READY`.
fn spawn_ready() -> (Child, BufReader<std::process::ChildStdout>) {
    let mut child = Command::new(DEMO)
        .env("DEMO_DRAIN_MS", DRAIN_MS.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn staged_shutdown_demo");

    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("read stdout");
        assert!(n > 0, "process exited before READY");
        if line.starts_with("READY") {
            break;
        }
    }
    (child, reader)
}

fn signal(pid: u32, sig: libc::c_int) {
    // Send the signal directly to the child PID (not via `cargo run`/a shell),
    // so it actually reaches the app; libc avoids depending on a `kill` binary.
    let rc = unsafe { libc::kill(pid as libc::pid_t, sig) };
    assert_eq!(rc, 0, "kill(pid={pid}, sig={sig}) failed");
}

/// SIGTERM -> ordered graceful drain (ingress, then worker, then publisher),
/// pre-stop delay honored, zero dropped requests, clean exit(0).
#[test]
fn sigterm_drains_stages_in_order_then_exits_cleanly() {
    let (mut child, mut reader) = spawn_ready();

    // Let it serve some load first.
    std::thread::sleep(Duration::from_millis(120));

    let sent = Instant::now();
    signal(child.id(), libc::SIGTERM);

    // Drain remaining output until the process closes stdout on exit.
    let mut out = String::new();
    reader
        .read_to_string(&mut out)
        .expect("read remaining stdout");
    let status = child.wait().expect("wait for child");
    let elapsed = sent.elapsed();

    assert!(
        status.success(),
        "clean exit expected after SIGTERM, got {status:?}\n---\n{out}"
    );

    // Ordered shutdown: ingress fully drains, then worker, then the publisher
    // it depends on.
    let ingress = out
        .find("INGRESS drained")
        .unwrap_or_else(|| panic!("no ingress drain\n{out}"));
    let worker = out
        .find("WORKER stopped")
        .unwrap_or_else(|| panic!("no worker stop\n{out}"));
    let publisher = out
        .find("PUBLISHER closed")
        .unwrap_or_else(|| panic!("no publisher close\n{out}"));
    assert!(
        ingress < worker && worker < publisher,
        "shutdown order wrong (ingress<worker<publisher expected)\n{out}"
    );

    // Zero dropped connections — nothing failed because a dependency (later
    // stage) shut down while requests were still in flight.
    assert!(
        out.contains("dropped=0"),
        "expected zero dropped requests\n{out}"
    );

    // The pre-stop drain delay was actually honored (not an instant teardown).
    assert!(
        elapsed >= Duration::from_millis(DRAIN_MS - 30),
        "drain delay not honored: shutdown took only {elapsed:?}\n{out}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "shutdown took too long: {elapsed:?}\n{out}"
    );
}

/// SIGKILL is the OS/ECS backstop: the process is terminated by signal 9 (no
/// graceful drain, no clean exit). Confirms the process is actually killable.
#[test]
fn sigkill_terminates_process_by_signal() {
    let (mut child, _reader) = spawn_ready();

    signal(child.id(), libc::SIGKILL);

    let status = child.wait().expect("wait for child");
    assert!(!status.success(), "SIGKILL must not be a clean exit");
    assert_eq!(
        status.signal(),
        Some(9),
        "process should be terminated by SIGKILL (signal 9), got {status:?}"
    );
}
