use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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
    env.command()
        .args([
            "init",
            "--db",
            custom_db.to_str().unwrap(),
            "--codex-root",
            custom_codex.to_str().unwrap(),
        ])
        .assert()
        .success();

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

#[test]
fn init_creates_sqlite_database_schema_and_is_idempotent() {
    let env = CliEnv::new();

    env.command().arg("init").assert().success().stdout(
        predicate::str::contains("sqlite: ok").and(predicate::str::contains(
            env.db_file().display().to_string(),
        )),
    );
    env.command().arg("init").assert().success();

    assert!(env.db_file().is_file());
    let conn = Connection::open(env.db_file()).unwrap();
    assert_eq!(user_version(&conn), 1);
    assert_table_exists(&conn, "schema_migrations");
    assert_table_exists(&conn, "codex_files");
    assert_table_exists(&conn, "sessions");
    assert_table_exists(&conn, "raw_events");
    assert_table_exists(&conn, "messages");
    assert_table_exists(&conn, "commands");
    assert_table_exists(&conn, "approvals");
    assert_table_exists(&conn, "file_changes");
    assert_table_exists(&conn, "proof_facts");
    assert_table_exists(&conn, "raw_events_fts");
    assert_table_exists(&conn, "messages_fts");
    assert_table_exists(&conn, "command_output_fts");

    let migration_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(migration_count, 1);
}

#[test]
fn doctor_reports_initialized_database_status() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();

    env.command().arg("doctor").assert().success().stdout(
        predicate::str::contains("sqlite: ok")
            .and(predicate::str::contains("migration: 1"))
            .and(predicate::str::contains("fts5: ok")),
    );
}

#[test]
fn doctor_does_not_create_missing_database() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();
    fs::remove_file(env.db_file()).unwrap();

    env.command()
        .arg("doctor")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to inspect database"));
    assert!(!env.db_file().exists());
}

#[test]
fn db_path_that_is_directory_reports_storage_error() {
    let env = CliEnv::new();
    let db_dir = env.home.path().join("db-is-directory");
    fs::create_dir_all(&db_dir).unwrap();

    env.command()
        .args(["init", "--db", db_dir.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("failed to initialize database")
                .and(predicate::str::contains(db_dir.display().to_string())),
        );
}

#[cfg(unix)]
#[test]
fn init_creates_config_and_db_with_owner_only_permissions() {
    let env = CliEnv::new();

    env.command().arg("init").assert().success();

    assert_eq!(file_mode(env.config_file()), 0o600);
    assert_eq!(file_mode(env.db_file()), 0o600);
}

#[cfg(unix)]
#[test]
fn doctor_warns_on_unsafe_config_and_db_permissions() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();
    fs::set_permissions(env.config_file(), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(env.db_file(), fs::Permissions::from_mode(0o666)).unwrap();

    env.command().arg("doctor").assert().success().stdout(
        predicate::str::contains("Warnings:")
            .and(predicate::str::contains("config permissions are 0644"))
            .and(predicate::str::contains("database permissions are 0666"))
            .and(predicate::str::contains("chmod 600")),
    );
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

fn assert_table_exists(conn: &Connection, table: &str) {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1, "expected table {table} to exist");
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[cfg(unix)]
fn file_mode(path: impl AsRef<std::path::Path>) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
