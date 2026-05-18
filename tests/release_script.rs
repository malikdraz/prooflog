use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"prooflog\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("Cargo.lock"),
        "[[package]]\nname = \"prooflog\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n## [1.2.3] - 2026-05-18\n\n- Existing release.\n",
    )
    .unwrap();
    dir
}

fn script_command(root: &TempDir) -> Command {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = repo_root.join("scripts").join("release.sh");
    let mut cmd = Command::new("bash");
    cmd.arg(script);
    cmd.env("PROOFLOG_RELEASE_ROOT", root.path());
    cmd
}

#[test]
fn release_script_computes_next_semver_versions() {
    let root = fixture();

    script_command(&root)
        .args(["next", "patch"])
        .assert()
        .success()
        .stdout("1.2.4\n");

    script_command(&root)
        .args(["next", "minor"])
        .assert()
        .success()
        .stdout("1.3.0\n");

    script_command(&root)
        .args(["next", "major"])
        .assert()
        .success()
        .stdout("2.0.0\n");
}

#[test]
fn release_script_rejects_invalid_tag_format() {
    let root = fixture();

    script_command(&root)
        .args(["verify-tag", "1.2.3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tag must match vX.Y.Z"));
}

#[test]
fn release_script_rejects_tag_version_mismatch() {
    let root = fixture();

    script_command(&root)
        .args(["verify-tag", "v1.2.4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not match Cargo.toml"));
}

#[test]
fn release_script_extracts_matching_changelog_section() {
    let root = fixture();
    let output = root.path().join("notes.md");

    script_command(&root)
        .args(["extract-notes", "v1.2.3", output.to_str().unwrap()])
        .assert()
        .success();

    let notes = fs::read_to_string(output).unwrap();
    assert!(notes.contains("## [1.2.3] - 2026-05-18"));
    assert!(notes.contains("- Existing release."));
    assert!(!notes.contains("Unreleased"));
}
