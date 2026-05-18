use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_mvp_commands() {
    let mut cmd = Command::cargo_bin("prooflog").unwrap();

    cmd.arg("--help").assert().success().stdout(
        predicate::str::contains("Local-first proof reports")
            .and(predicate::str::contains("init"))
            .and(predicate::str::contains("doctor"))
            .and(predicate::str::contains("ingest"))
            .and(predicate::str::contains("proof")),
    );
}

#[test]
fn proof_requires_since_argument() {
    let mut cmd = Command::cargo_bin("prooflog").unwrap();

    cmd.arg("proof")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--since"));
}

#[test]
fn ingest_requires_source_flag() {
    let mut cmd = Command::cargo_bin("prooflog").unwrap();

    cmd.arg("ingest")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--codex"));
}

#[test]
fn placeholder_commands_are_explicit_and_non_mutating() {
    let mut init = Command::cargo_bin("prooflog").unwrap();
    init.arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("not implemented yet"));

    let mut doctor = Command::cargo_bin("prooflog").unwrap();
    doctor
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("not implemented yet"));

    let mut ingest = Command::cargo_bin("prooflog").unwrap();
    ingest
        .args(["ingest", "--codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not implemented yet"));

    let mut proof = Command::cargo_bin("prooflog").unwrap();
    proof
        .args(["proof", "--since", "main"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not implemented yet"));
}
