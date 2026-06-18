//! Regression test for the CLI out-of-the-box mutation fix.
//!
//! Previously every mutating command ran as the capability-less `anonymous`
//! identity, so `prompthub add` failed with
//! `Unauthorized: agent 'anonymous' lacks capability Write`. The CLI now acts
//! as a trusted local operator (Read + Write + Admin), so `add` works on a
//! fresh store without any configuration.

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn add_succeeds_out_of_the_box() {
    // Fresh dir → a brand-new `prompthub.db`; no config, no identity setup.
    let dir = tempdir().expect("tempdir");

    Command::cargo_bin("prompthub")
        .expect("prompthub binary")
        .current_dir(dir.path())
        .arg("add")
        .assert()
        .success()
        .stdout(predicates::str::contains("Registered prompt"));
}

#[test]
fn add_attributes_to_named_operator_via_env() {
    let dir = tempdir().expect("tempdir");

    // PROMPTHUB_AGENT overrides the operator's display name; the command must
    // still succeed (the capability grant is independent of the name).
    Command::cargo_bin("prompthub")
        .expect("prompthub binary")
        .current_dir(dir.path())
        .env("PROMPTHUB_AGENT", "ci-bot")
        .arg("add")
        .assert()
        .success();
}
