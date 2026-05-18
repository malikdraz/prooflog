use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
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
fn proof_command_is_explicitly_unimplemented() {
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
    assert_eq!(user_version(&conn), 2);
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
    assert_eq!(migration_count, 2);
}

#[test]
fn doctor_reports_initialized_database_status() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();

    env.command().arg("doctor").assert().success().stdout(
        predicate::str::contains("sqlite: ok")
            .and(predicate::str::contains("migration: 2"))
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

#[test]
fn doctor_warns_when_codex_root_or_git_repo_are_missing() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();

    env.command_in(env.home.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Codex:")
                .and(predicate::str::contains("root: missing"))
                .and(predicate::str::contains("jsonl files: 0"))
                .and(predicate::str::contains("Git:"))
                .and(predicate::str::contains("repo: not detected"))
                .and(predicate::str::contains("Warnings:"))
                .and(predicate::str::contains("Codex root does not exist"))
                .and(predicate::str::contains("not inside a git repo")),
        );
}

#[test]
fn doctor_reports_codex_jsonl_count_and_current_git_repo() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join(".codex");
    fs::create_dir_all(codex_root.join("sessions")).unwrap();
    fs::write(codex_root.join("session-a.jsonl"), "{}\n").unwrap();
    fs::write(codex_root.join("sessions").join("session-b.jsonl"), "{}\n").unwrap();
    fs::write(codex_root.join("ignore.txt"), "not jsonl").unwrap();

    env.command().arg("init").assert().success();

    env.command().arg("doctor").assert().success().stdout(
        predicate::str::contains("Codex:")
            .and(predicate::str::contains("root: ok"))
            .and(predicate::str::contains("jsonl files: 2"))
            .and(predicate::str::contains("Git:"))
            .and(predicate::str::contains("repo: "))
            .and(predicate::str::contains("branch: ")),
    );
}

#[test]
fn ingest_discovers_jsonl_files_and_records_metadata() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(codex_root.join("nested")).unwrap();
    fs::write(codex_root.join("session-a.jsonl"), "{}\n").unwrap();
    fs::write(
        codex_root.join("nested").join("session-b.jsonl"),
        "{\"x\":1}\n",
    )
    .unwrap();
    fs::write(codex_root.join("ignore.txt"), "not jsonl").unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("files discovered: 2")
                .and(predicate::str::contains("files ingested: 2"))
                .and(predicate::str::contains("warnings: 0")),
        );

    let rows = codex_file_rows(env.db_file());
    assert_eq!(rows.len(), 2);
    let first = rows
        .iter()
        .find(|row| row.path.ends_with("session-a.jsonl"))
        .unwrap();
    assert_eq!(first.size_bytes, 3);
    assert_eq!(first.sha256, sha256_hex("{}\n"));
    assert!(!first.modified_at.is_empty());
}

#[test]
fn repeated_ingest_skips_unchanged_and_updates_changed_files() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let file = codex_root.join("session.jsonl");
    fs::write(&file, "{}\n").unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("files ingested: 0")
                .and(predicate::str::contains("files skipped: 1")),
        );

    fs::write(&file, "{\"changed\":true}\n").unwrap();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("files ingested: 1")
                .and(predicate::str::contains("files skipped: 0")),
        );

    let rows = codex_file_rows(env.db_file());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].sha256, sha256_hex("{\"changed\":true}\n"));
}

#[test]
fn ingest_stores_raw_jsonl_lines_and_parse_metadata() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let file = codex_root.join("session.jsonl");
    fs::write(
        &file,
        "{\"type\":\"session_started\",\"timestamp\":\"2026-05-18T10:00:00Z\"}\n{\"unknown\":true}\nnot json\n\n",
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("raw events stored: 3")
                .and(predicate::str::contains("raw events skipped: 1"))
                .and(predicate::str::contains("malformed lines: 1")),
        );

    let rows = raw_event_rows(env.db_file());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].line_number, 1);
    assert_eq!(
        rows[0].raw_json,
        "{\"type\":\"session_started\",\"timestamp\":\"2026-05-18T10:00:00Z\"}"
    );
    assert_eq!(rows[0].line_sha256, sha256_hex(&rows[0].raw_json));
    assert_eq!(rows[0].event_type.as_deref(), Some("session_started"));
    assert_eq!(rows[0].event_time.as_deref(), Some("2026-05-18T10:00:00Z"));
    assert!(rows[0].parse_error.is_none());
    assert_eq!(rows[1].line_number, 2);
    assert_eq!(rows[1].raw_json, "{\"unknown\":true}");
    assert!(rows[1].event_type.is_none());
    assert!(rows[1].event_time.is_none());
    assert!(rows[1].parse_error.is_none());
    assert_eq!(rows[2].line_number, 3);
    assert_eq!(rows[2].raw_json, "not json");
    assert!(rows[2].parse_error.as_deref().unwrap().contains("expected"));
}

#[test]
fn ingest_summary_reports_mixed_raw_event_counts_without_warning_details_when_clean() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("session.jsonl"),
        "{\"type\":\"message\"}\n{\"unknown\":true}\nnot json\n\n",
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("files discovered: 1")
                .and(predicate::str::contains("files ingested: 1"))
                .and(predicate::str::contains("files skipped: 0"))
                .and(predicate::str::contains("raw events stored: 3"))
                .and(predicate::str::contains("raw events skipped: 1"))
                .and(predicate::str::contains("malformed lines: 1"))
                .and(predicate::str::contains("unknown event shapes: 1"))
                .and(predicate::str::contains("warnings: 0"))
                .and(predicate::str::contains("Warnings:").not()),
        );
}

#[test]
fn repeated_ingest_does_not_duplicate_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(codex_root.join("session.jsonl"), "{\"type\":\"a\"}\n").unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    let rows = raw_event_rows(env.db_file());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw_json, "{\"type\":\"a\"}");
}

#[test]
fn ingest_preserves_duplicate_physical_lines() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(codex_root.join("session.jsonl"), "{}\n{}\n").unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("raw events stored: 2"));

    let rows = raw_event_rows(env.db_file());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].line_number, 1);
    assert_eq!(rows[1].line_number, 2);
    assert_eq!(rows[0].line_sha256, rows[1].line_sha256);
}

#[test]
fn ingest_migrates_existing_v1_database_before_storing_duplicate_lines() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(codex_root.join("session.jsonl"), "{}\n{}\n").unwrap();

    env.command().arg("init").assert().success();
    fs::remove_file(env.db_file()).unwrap();
    create_v1_database(env.db_file());

    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("raw events stored: 2"));

    let conn = Connection::open(env.db_file()).unwrap();
    assert_eq!(user_version(&conn), 2);
    assert_eq!(raw_event_rows(env.db_file()).len(), 2);
}

#[test]
fn ingest_populates_raw_event_fts_index() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("session.jsonl"),
        "{\"type\":\"message\",\"body\":\"needle_token\"}\nnot json\n",
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(raw_event_fts_match_count(env.db_file(), "needle_token"), 1);
    assert_eq!(raw_event_fts_match_count(env.db_file(), "expected"), 1);
}

#[test]
fn repeated_ingest_keeps_raw_event_fts_matches_stable() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(codex_root.join("session.jsonl"), "{\"body\":\"stable\"}\n").unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    assert_eq!(raw_event_fts_match_count(env.db_file(), "stable"), 1);
}

#[test]
fn changed_jsonl_line_refreshes_raw_event_fts_index() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let file = codex_root.join("session.jsonl");
    fs::write(&file, "{\"body\":\"before_token\"}\n").unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    fs::write(&file, "{\"body\":\"after_token\"}\n").unwrap();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(raw_event_fts_match_count(env.db_file(), "before_token"), 0);
    assert_eq!(raw_event_fts_match_count(env.db_file(), "after_token"), 1);
}

#[test]
fn ingest_derives_sessions_from_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("01_single_success.jsonl"),
        include_str!("fixtures/codex/01_single_success.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let sessions = session_rows(env.db_file());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].codex_session_id, "session-redacted-001");
    assert_eq!(
        sessions[0].workspace_path.as_deref(),
        Some("/workspace/prooflog")
    );
    assert_eq!(sessions[0].model.as_deref(), Some("gpt-5"));
    assert_eq!(
        sessions[0].title.as_deref(),
        Some("Add focused ProofLog test")
    );
    assert_eq!(
        sessions[0].started_at.as_deref(),
        Some("2026-05-18T10:00:00Z")
    );
    assert_eq!(
        sessions[0].ended_at.as_deref(),
        Some("2026-05-18T10:01:00Z")
    );
    assert_eq!(sessions[0].event_count, 4);
    assert_eq!(sessions[0].parse_status, "parsed");
    assert_eq!(linked_raw_event_count(env.db_file(), sessions[0].id), 4);
}

#[test]
fn repeated_ingest_does_not_duplicate_sessions() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("01_single_success.jsonl"),
        include_str!("fixtures/codex/01_single_success.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    assert_eq!(session_rows(env.db_file()).len(), 1);
}

#[test]
fn session_derivation_allows_missing_optional_fields() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("minimal.jsonl"),
        "{\"type\":\"session_meta\",\"session_id\":\"minimal-session\"}\nnot json\n",
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let sessions = session_rows(env.db_file());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].codex_session_id, "minimal-session");
    assert!(sessions[0].workspace_path.is_none());
    assert!(sessions[0].model.is_none());
    assert!(sessions[0].title.is_none());
    assert!(sessions[0].started_at.is_none());
    assert!(sessions[0].ended_at.is_none());
    assert_eq!(sessions[0].event_count, 1);
    assert_eq!(sessions[0].parse_status, "parsed");
}

#[test]
fn ingest_derives_messages_from_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("01_single_success.jsonl"),
        include_str!("fixtures/codex/01_single_success.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let messages = message_rows(env.db_file());
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| message.raw_event_id > 0));
    assert!(messages
        .iter()
        .all(|message| message.session_id == Some(messages[0].session_id.unwrap())));
    assert_eq!(messages[0].role, "user");
    assert_eq!(
        messages[0].text,
        "Add a focused test for the ProofLog ingest summary and verify it passes."
    );
    assert_eq!(
        messages[0].created_at.as_deref(),
        Some("2026-05-18T10:00:05Z")
    );
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(
        messages[1].text,
        "I added the focused ingest summary test and will run the Rust test suite."
    );
    assert_eq!(
        messages[1].created_at.as_deref(),
        Some("2026-05-18T10:00:20Z")
    );
}

#[test]
fn repeated_ingest_does_not_duplicate_messages() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("02_fail_then_pass.jsonl"),
        include_str!("fixtures/codex/02_fail_then_pass.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    assert_eq!(message_rows(env.db_file()).len(), 3);
}

#[test]
fn message_derivation_skips_empty_and_unknown_shapes() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("messages.jsonl"),
        concat!(
            "{\"type\":\"message\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"   \"}}\n",
            "{\"type\":\"message\",\"timestamp\":\"2026-05-18T12:00:05Z\",\"message\":{\"content\":\"missing role\"}}\n",
            "{\"type\":\"message\",\"timestamp\":\"2026-05-18T12:00:10Z\",\"message\":{\"role\":\"assistant\",\"content\":\"Visible answer\"}}\n",
            "not json\n"
        ),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let messages = message_rows(env.db_file());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "assistant");
    assert_eq!(messages[0].text, "Visible answer");
    assert!(messages[0].session_id.is_none());
}

#[test]
fn ingest_populates_message_fts_index() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("01_single_success.jsonl"),
        include_str!("fixtures/codex/01_single_success.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(message_fts_match_count(env.db_file(), "verify"), 1);
}

#[test]
fn ingest_derives_commands_from_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("01_single_success.jsonl"),
        include_str!("fixtures/codex/01_single_success.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let commands = command_rows(env.db_file());
    assert_eq!(commands.len(), 1);
    assert!(commands[0].raw_event_id > 0);
    assert!(commands[0].session_id.is_some());
    assert_eq!(commands[0].command, "cargo test");
    assert_eq!(commands[0].cwd.as_deref(), Some("/workspace/prooflog"));
    assert_eq!(commands[0].status.as_deref(), Some("success"));
    assert_eq!(commands[0].exit_code, Some(0));
    assert_eq!(
        commands[0].output.as_deref(),
        Some("test result: ok. 32 passed; 0 failed")
    );
    assert_eq!(
        commands[0].started_at.as_deref(),
        Some("2026-05-18T10:01:00Z")
    );
    assert_eq!(
        commands[0].ended_at.as_deref(),
        Some("2026-05-18T10:01:00Z")
    );
}

#[test]
fn command_derivation_extracts_failed_commands() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("03_unresolved_failure.jsonl"),
        include_str!("fixtures/codex/03_unresolved_failure.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let commands = command_rows(env.db_file());
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].status.as_deref(), Some("failure"));
    assert_eq!(commands[0].exit_code, Some(101));
    assert_eq!(
        commands[0].output.as_deref(),
        Some("test fixture_03_unresolved_failure_is_not_ready ... FAILED")
    );
}

#[test]
fn command_derivation_skips_unknown_shapes() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"command\":{\"cwd\":\"/workspace/prooflog\",\"status\":\"success\"}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:05Z\",\"command\":{\"cmd\":\"cargo test\"}}\n",
            "not json\n"
        ),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let commands = command_rows(env.db_file());
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command, "cargo test");
    assert!(commands[0].session_id.is_none());
    assert!(commands[0].status.is_none());
    assert!(commands[0].exit_code.is_none());
}

#[test]
fn repeated_ingest_does_not_duplicate_commands() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("02_fail_then_pass.jsonl"),
        include_str!("fixtures/codex/02_fail_then_pass.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    assert_eq!(command_rows(env.db_file()).len(), 2);
}

#[test]
fn ingest_populates_command_output_fts_index() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("03_unresolved_failure.jsonl"),
        include_str!("fixtures/codex/03_unresolved_failure.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(command_output_fts_match_count(env.db_file(), "FAILED"), 1);
}

#[test]
fn ingest_derives_approvals_from_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("04_approval_risk.jsonl"),
        include_str!("fixtures/codex/04_approval_risk.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let approvals = approval_rows(env.db_file());
    assert_eq!(approvals.len(), 1);
    assert!(approvals[0].raw_event_id > 0);
    assert!(approvals[0].session_id.is_some());
    assert_eq!(approvals[0].action.as_deref(), Some("run_command"));
    assert_eq!(approvals[0].decision.as_deref(), Some("approved"));
    assert_eq!(
        approvals[0].sandbox_mode.as_deref(),
        Some("network-restricted")
    );
    assert_eq!(
        approvals[0].command.as_deref(),
        Some("gh pr checks --repo example-org/example-repo")
    );
    assert_eq!(
        approvals[0].created_at.as_deref(),
        Some("2026-05-18T13:00:30Z")
    );
}

#[test]
fn approval_derivation_allows_missing_optional_fields() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("approvals.jsonl"),
        concat!(
            "{\"type\":\"approval\",\"timestamp\":\"2026-05-18T13:00:30Z\",\"approval\":{}}\n",
            "{\"type\":\"approval\",\"timestamp\":\"2026-05-18T13:00:40Z\"}\n",
            "not json\n"
        ),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let approvals = approval_rows(env.db_file());
    assert_eq!(approvals.len(), 1);
    assert!(approvals[0].session_id.is_none());
    assert!(approvals[0].action.is_none());
    assert!(approvals[0].decision.is_none());
    assert!(approvals[0].sandbox_mode.is_none());
    assert!(approvals[0].command.is_none());
    assert_eq!(
        approvals[0].created_at.as_deref(),
        Some("2026-05-18T13:00:30Z")
    );
}

#[test]
fn repeated_ingest_does_not_duplicate_approvals() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("04_approval_risk.jsonl"),
        include_str!("fixtures/codex/04_approval_risk.jsonl"),
    )
    .unwrap();

    env.command().arg("init").assert().success();
    for _ in 0..2 {
        env.command()
            .args([
                "ingest",
                "--codex",
                "--codex-root",
                codex_root.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    assert_eq!(approval_rows(env.db_file()).len(), 1);
}

#[test]
fn doctor_reports_missing_raw_event_fts_table() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();
    let conn = Connection::open(env.db_file()).unwrap();
    conn.execute("DROP TABLE raw_events_fts", []).unwrap();

    env.command().arg("doctor").assert().failure().stderr(
        predicate::str::contains("failed to inspect database")
            .and(predicate::str::contains("raw_events_fts")),
    );
}

#[test]
fn changed_jsonl_line_updates_existing_raw_event_row() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let file = codex_root.join("session.jsonl");
    fs::write(&file, "{\"type\":\"before\"}\n").unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success();

    fs::write(&file, "{\"type\":\"after\"}\n").unwrap();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("raw events stored: 1"));

    let rows = raw_event_rows(env.db_file());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw_json, "{\"type\":\"after\"}");
    assert_eq!(rows[0].event_type.as_deref(), Some("after"));
}

#[test]
fn ingest_missing_codex_root_is_actionable_error() {
    let env = CliEnv::new();
    let missing_root = env.home.path().join("missing-codex");
    env.command().arg("init").assert().success();

    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            missing_root.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Codex root does not exist")
                .and(predicate::str::contains(missing_root.display().to_string())),
        );
}

#[cfg(unix)]
#[test]
fn ingest_skips_symlinked_directories() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    let real = codex_root.join("real");
    let linked = codex_root.join("linked");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("session.jsonl"), "{}\n").unwrap();
    symlink(&codex_root, &linked).unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("files discovered: 1"));

    assert_eq!(codex_file_rows(env.db_file()).len(), 1);
}

#[cfg(unix)]
#[test]
fn ingest_reports_and_skips_unreadable_jsonl_files() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let readable = codex_root.join("readable.jsonl");
    let unreadable = codex_root.join("unreadable.jsonl");
    fs::write(&readable, "{}\n").unwrap();
    fs::write(&unreadable, "{}\n").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

    env.command().arg("init").assert().success();
    env.command()
        .args([
            "ingest",
            "--codex",
            "--codex-root",
            codex_root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("files discovered: 1")
                .and(predicate::str::contains("warnings: 1"))
                .and(predicate::str::contains("Warnings:"))
                .and(predicate::str::contains("could not read")),
        );

    assert_eq!(codex_file_rows(env.db_file()).len(), 1);
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
        if let Some(path) = std::env::var_os("PATH") {
            cmd.env("PATH", path);
        }
        cmd
    }

    fn command_in(&self, cwd: impl AsRef<std::path::Path>) -> Command {
        let mut cmd = self.command();
        cmd.current_dir(cwd);
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

struct CodexFileRow {
    path: String,
    size_bytes: i64,
    modified_at: String,
    sha256: String,
}

struct RawEventRow {
    line_number: i64,
    raw_json: String,
    line_sha256: String,
    event_type: Option<String>,
    event_time: Option<String>,
    parse_error: Option<String>,
}

struct SessionRow {
    id: i64,
    codex_session_id: String,
    workspace_path: Option<String>,
    model: Option<String>,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    event_count: i64,
    parse_status: String,
}

struct MessageRow {
    raw_event_id: i64,
    session_id: Option<i64>,
    role: String,
    text: String,
    created_at: Option<String>,
}

struct CommandRow {
    raw_event_id: i64,
    session_id: Option<i64>,
    command: String,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    output: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
}

struct ApprovalRow {
    raw_event_id: i64,
    session_id: Option<i64>,
    action: Option<String>,
    decision: Option<String>,
    sandbox_mode: Option<String>,
    command: Option<String>,
    created_at: Option<String>,
}

fn codex_file_rows(db_path: impl AsRef<std::path::Path>) -> Vec<CodexFileRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT path, size_bytes, modified_at, sha256 FROM codex_files ORDER BY path")
        .unwrap();
    stmt.query_map([], |row| {
        Ok(CodexFileRow {
            path: row.get(0)?,
            size_bytes: row.get(1)?,
            modified_at: row.get(2)?,
            sha256: row.get(3)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn raw_event_rows(db_path: impl AsRef<std::path::Path>) -> Vec<RawEventRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT line_number, raw_json, line_sha256, event_type, event_time, parse_error
             FROM raw_events
             ORDER BY codex_file_id, line_number",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(RawEventRow {
            line_number: row.get(0)?,
            raw_json: row.get(1)?,
            line_sha256: row.get(2)?,
            event_type: row.get(3)?,
            event_time: row.get(4)?,
            parse_error: row.get(5)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn session_rows(db_path: impl AsRef<std::path::Path>) -> Vec<SessionRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, codex_session_id, workspace_path, model, title, started_at, ended_at, event_count, parse_status
             FROM sessions
             ORDER BY codex_session_id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            codex_session_id: row.get(1)?,
            workspace_path: row.get(2)?,
            model: row.get(3)?,
            title: row.get(4)?,
            started_at: row.get(5)?,
            ended_at: row.get(6)?,
            event_count: row.get(7)?,
            parse_status: row.get(8)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn message_rows(db_path: impl AsRef<std::path::Path>) -> Vec<MessageRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT raw_event_id, session_id, role, text, created_at
             FROM messages
             ORDER BY raw_event_id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(MessageRow {
            raw_event_id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            text: row.get(3)?,
            created_at: row.get(4)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn command_rows(db_path: impl AsRef<std::path::Path>) -> Vec<CommandRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT raw_event_id, session_id, command, cwd, status, exit_code, output, started_at, ended_at
             FROM commands
             ORDER BY raw_event_id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(CommandRow {
            raw_event_id: row.get(0)?,
            session_id: row.get(1)?,
            command: row.get(2)?,
            cwd: row.get(3)?,
            status: row.get(4)?,
            exit_code: row.get(5)?,
            output: row.get(6)?,
            started_at: row.get(7)?,
            ended_at: row.get(8)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn approval_rows(db_path: impl AsRef<std::path::Path>) -> Vec<ApprovalRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT raw_event_id, session_id, action, decision, sandbox_mode, command, created_at
             FROM approvals
             ORDER BY raw_event_id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(ApprovalRow {
            raw_event_id: row.get(0)?,
            session_id: row.get(1)?,
            action: row.get(2)?,
            decision: row.get(3)?,
            sandbox_mode: row.get(4)?,
            command: row.get(5)?,
            created_at: row.get(6)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn linked_raw_event_count(db_path: impl AsRef<std::path::Path>, session_id: i64) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM raw_events WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )
    .unwrap()
}

fn message_fts_match_count(db_path: impl AsRef<std::path::Path>, query: &str) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1",
        [query],
        |row| row.get(0),
    )
    .unwrap()
}

fn command_output_fts_match_count(db_path: impl AsRef<std::path::Path>, query: &str) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM command_output_fts WHERE command_output_fts MATCH ?1",
        [query],
        |row| row.get(0),
    )
    .unwrap()
}

fn raw_event_fts_match_count(db_path: impl AsRef<std::path::Path>, query: &str) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM raw_events_fts WHERE raw_events_fts MATCH ?1",
        [query],
        |row| row.get(0),
    )
    .unwrap()
}

fn create_v1_database(db_path: impl AsRef<std::path::Path>) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        r#"
        PRAGMA user_version = 1;

        CREATE TABLE schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        INSERT INTO schema_migrations (version) VALUES (1);

        CREATE TABLE codex_files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            size_bytes INTEGER,
            modified_at TEXT,
            sha256 TEXT,
            ingested_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE sessions (
            id INTEGER PRIMARY KEY,
            codex_session_id TEXT UNIQUE,
            workspace_path TEXT,
            model TEXT,
            title TEXT,
            started_at TEXT,
            ended_at TEXT,
            event_count INTEGER NOT NULL DEFAULT 0,
            parse_status TEXT NOT NULL DEFAULT 'unknown'
        );

        CREATE TABLE raw_events (
            id INTEGER PRIMARY KEY,
            codex_file_id INTEGER NOT NULL REFERENCES codex_files(id) ON DELETE CASCADE,
            line_number INTEGER NOT NULL,
            raw_json TEXT NOT NULL,
            line_sha256 TEXT NOT NULL UNIQUE,
            event_type TEXT,
            event_time TEXT,
            session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
            parse_error TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (codex_file_id, line_number)
        );
        "#,
    )
    .unwrap();
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
