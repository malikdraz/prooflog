use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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
fn unimplemented_commands_are_explicit_and_non_mutating() {
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

#[test]
fn init_creates_config_with_resolved_local_paths() {
    let env = CliEnv::new();

    let mut cmd = env.command();
    cmd.arg("init")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Config:").and(predicate::str::contains(
                env.config_file().display().to_string(),
            )),
        );

    let config = fs::read_to_string(env.config_file()).unwrap();
    assert!(config.contains(&format!("db_path = \"{}\"", env.db_file().display())));
    assert!(config.contains(&format!(
        "codex_root = \"{}\"",
        env.home.path().join(".codex").display()
    )));
    assert!(config.contains("redact_secrets = true"));
}

#[test]
fn doctor_reads_config_and_applies_cli_overrides() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();

    let custom_db = env.home.path().join("custom-prooflog.db");
    let custom_codex = env.home.path().join("codex-history");

    let mut cmd = env.command();
    cmd.args([
        "doctor",
        "--db",
        custom_db.to_str().unwrap(),
        "--codex-root",
        custom_codex.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(
        predicate::str::contains("config ok")
            .and(predicate::str::contains(custom_db.display().to_string()))
            .and(predicate::str::contains(custom_codex.display().to_string())),
    );
}

#[test]
fn invalid_config_path_reports_actionable_error() {
    let env = CliEnv::new();
    fs::create_dir_all(env.config_file()).unwrap();

    let mut cmd = env.command();
    cmd.arg("init")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("invalid config path").and(predicate::str::contains(
                env.config_file().display().to_string(),
            )),
        );
}

#[test]
fn missing_home_reports_actionable_error() {
    let mut cmd = Command::cargo_bin("prooflog").unwrap();
    cmd.env_clear()
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("HOME is not set"));
}

#[test]
fn missing_xdg_vars_falls_back_under_home() {
    let home = tempfile::tempdir().unwrap();
    let config_file = home
        .path()
        .join(".config")
        .join("prooflog")
        .join("config.toml");
    let db_file = home
        .path()
        .join(".local")
        .join("share")
        .join("prooflog")
        .join("prooflog.db");

    let mut cmd = Command::cargo_bin("prooflog").unwrap();
    cmd.env_clear()
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(config_file.display().to_string()));

    let config = fs::read_to_string(config_file).unwrap();
    assert!(config.contains(&format!("db_path = \"{}\"", db_file.display())));
}

struct CliEnv {
    home: TempDir,
    config_home: TempDir,
    data_home: TempDir,
}

impl CliEnv {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            config_home: tempfile::tempdir().unwrap(),
            data_home: tempfile::tempdir().unwrap(),
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::cargo_bin("prooflog").unwrap();
        cmd.env_clear()
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.config_home.path())
            .env("XDG_DATA_HOME", self.data_home.path());
        cmd
    }

    fn config_file(&self) -> std::path::PathBuf {
        self.config_home.path().join("prooflog").join("config.toml")
    }

    fn db_file(&self) -> std::path::PathBuf {
        self.data_home.path().join("prooflog").join("prooflog.db")
    }
}
