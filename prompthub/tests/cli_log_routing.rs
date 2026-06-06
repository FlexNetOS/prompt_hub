//! Regression test for the CLI log-routing fix.
//!
//! Tracing logs must go to **stderr** so that **stdout** stays reserved for
//! machine-readable command output. This encodes the `/verify` finding that
//! `prompthub metrics` previously mixed ANSI-colored INFO lines into the
//! Prometheus exposition on stdout, breaking any Prometheus parser fed via
//! `prompthub metrics > out.prom`.
//!
//! Gated behind `otel` because the `metrics` subcommand only exists with that
//! feature (matching the `metrics` command's own gating).
#![cfg(feature = "otel")]

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn metrics_stdout_is_clean_prometheus_and_logs_go_to_stderr() {
    // Run in a throwaway dir so the `prompthub.db` the command opens does not
    // pollute the repo.
    let dir = tempdir().expect("tempdir");

    let assert = Command::cargo_bin("prompthub")
        .expect("prompthub binary")
        .current_dir(dir.path())
        .arg("metrics")
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout must be a valid Prometheus exposition: the first non-empty line is
    // a `# HELP`/`# TYPE` directive.
    let first_line = stdout.lines().find(|l| !l.is_empty()).unwrap_or_default();
    assert!(
        first_line.starts_with("# HELP") || first_line.starts_with("# TYPE"),
        "stdout should begin with the Prometheus preamble, got {first_line:?}\n\
         --- full stdout ---\n{stdout}"
    );

    // No tracing log lines or ANSI escape codes may leak onto stdout.
    assert!(
        !stdout.contains("INFO") && !stdout.contains('\u{1b}'),
        "stdout leaked tracing logs or ANSI escapes:\n{stdout}"
    );

    // The `info!` emitted by the metrics command must have been routed to
    // stderr instead. (stderr is a pipe here, so `with_ansi` is off → plain
    // text, matching what a redirected log file would contain.)
    assert!(
        stderr.contains("Rendering Prometheus metrics exposition"),
        "expected the INFO log on stderr, got:\n{stderr}"
    );
}
