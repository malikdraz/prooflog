use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::{fs, process::Command as ProcessCommand};
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
fn proof_prints_plain_text_report_sections() {
    let mut proof = Command::cargo_bin("prooflog").unwrap();
    proof
        .args(["proof", "--since", "main"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("PROOFLOG REPORT")
                .and(predicate::str::contains("Scope:"))
                .and(predicate::str::contains("Changed:"))
                .and(predicate::str::contains("Codex evidence:"))
                .and(predicate::str::contains("Verification:"))
                .and(predicate::str::contains("Failures:"))
                .and(predicate::str::contains("Risks:"))
                .and(predicate::str::contains("Decision:"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains("not implemented yet").not()),
        );
}

#[test]
fn proof_accepts_explicit_text_format() {
    let mut proof = Command::cargo_bin("prooflog").unwrap();
    proof
        .args(["proof", "--since", "main", "--format", "text"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("PROOFLOG REPORT")
                .and(predicate::str::contains("Scope:"))
                .and(predicate::str::contains("# ProofLog Report").not()),
        );
}

#[test]
fn proof_rejects_unknown_format() {
    let mut proof = Command::cargo_bin("prooflog").unwrap();
    proof
        .args(["proof", "--since", "main", "--format", "xml"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("invalid value")
                .and(predicate::str::contains("xml"))
                .and(predicate::str::contains("text"))
                .and(predicate::str::contains("md"))
                .and(predicate::str::contains("json")),
        );
}

#[test]
fn proof_json_format_is_machine_readable() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src").join("auth")).unwrap();
    fs::write(
        repo.join("src").join("auth").join("session.rs"),
        "pub fn auth() {}\n",
    )
    .unwrap();
    git(&repo, ["add", "src/auth/session.rs"]);
    git(&repo, ["commit", "-m", "add auth"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("json-ready.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T16:10:00Z\",\"session_id\":\"session-json\",\"workspace_path\":\"{}\",\"title\":\"JSON proof\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T16:10:05Z\",\"session_id\":\"session-json\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"SECRET_JSON_OUTPUT should not print\"}}}}\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    let output = env
        .command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--format",
            "json",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("SECRET_JSON_OUTPUT"));
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["format"], "json");
    assert_eq!(report["scope"]["since"], "main~1");
    assert_eq!(report["changed"]["files_count"], 1);
    assert_eq!(report["changed"]["files"][0]["path"], "src/auth/session.rs");
    assert_eq!(report["codex"]["relevant_sessions_count"], 1);
    assert_eq!(
        report["codex"]["relevant_sessions"][0]["id"],
        "session-json"
    );
    assert_eq!(report["verification"]["passed"], 1);
    assert_eq!(report["risks"]["changed_paths"]["level"], "elevated");
    assert_eq!(report["decision"]["status"], "READY");
    assert_eq!(
        report["next_actions"][0],
        "review and paste the report where proof is needed"
    );
}

#[test]
fn proof_reports_parser_warning_counts_without_raw_events() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("parser-warnings.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T16:20:00Z\",\"session_id\":\"session-parser-warnings\",\"workspace_path\":\"{}\",\"title\":\"Parser warnings\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T16:20:05Z\",\"session_id\":\"session-parser-warnings\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"ok\"}}}}\n\
             {{\"mystery\":true}}\n\
             not json SECRET_PARSE_OUTPUT\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Parser warnings:")
                .and(predicate::str::contains("malformed lines: 1"))
                .and(predicate::str::contains("unknown event shapes: 1"))
                .and(predicate::str::contains("SECRET_PARSE_OUTPUT").not()),
        );

    let output = env
        .command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--format",
            "json",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("SECRET_PARSE_OUTPUT"));
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["parser_warnings"]["malformed_lines"], 1);
    assert_eq!(report["parser_warnings"]["unknown_event_shapes"], 1);
}

#[test]
fn proof_redacts_report_secrets_but_preserves_raw_command() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    let openai_key = "sk-proj-abcdefghijklmnopqrstuvwxyz1234567890";
    let bearer = "Bearer bearerSECRET1234567890";
    let command = format!("cargo test -- --api-key {openai_key} --header '{bearer}'");
    fs::write(
        codex_root.join("secret-command.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T16:30:00Z\",\"session_id\":\"session-secret\",\"workspace_path\":\"{}\",\"title\":\"Secret report\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T16:30:05Z\",\"session_id\":\"session-secret\",\"command\":{{\"cmd\":\"{}\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"ok\"}}}}\n",
            repo_root.display(),
            command,
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    let rows = command_rows(env.db_file());
    assert_eq!(rows.len(), 1);
    assert!(rows[0].command.contains(openai_key));
    assert!(rows[0].command.contains(bearer));

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[REDACTED_SECRET]")
                .and(predicate::str::contains(openai_key).not())
                .and(predicate::str::contains(bearer).not()),
        );

    let output = env
        .command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--format",
            "json",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("[REDACTED_SECRET]"));
    assert!(!stdout.contains(openai_key));
    assert!(!stdout.contains(bearer));
}

#[test]
fn proof_markdown_format_is_snapshot_covered() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src").join("auth")).unwrap();
    fs::write(
        repo.join("src").join("auth").join("session.rs"),
        "pub fn auth() {}\n",
    )
    .unwrap();
    git(&repo, ["add", "src/auth/session.rs"]);
    git(&repo, ["commit", "-m", "add auth"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("markdown-ready.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T16:00:00Z\",\"session_id\":\"session-md\",\"workspace_path\":\"{}\",\"title\":\"Markdown proof\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T16:00:05Z\",\"session_id\":\"session-md\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"SECRET_MARKDOWN_OUTPUT should not print\"}}}}\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    let output = env
        .command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--format",
            "md",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("SECRET_MARKDOWN_OUTPUT"));
    insta::assert_snapshot!(normalize_report_output(&stdout, &repo));
}

#[test]
fn proof_reports_git_context_for_repo_override() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::write(repo.join("untracked.txt"), "pending\n").unwrap();

    env.command_in(env.home.path())
        .args(["proof", "--since", "main", "--repo", repo.to_str().unwrap()])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Scope:")
                .and(predicate::str::contains(format!(
                    "repo: {}",
                    repo.canonicalize().unwrap().display()
                )))
                .and(predicate::str::contains("branch: main"))
                .and(predicate::str::contains("head: "))
                .and(predicate::str::contains("merge base: "))
                .and(predicate::str::contains("dirty: yes")),
        );
}

#[test]
fn proof_detects_git_context_from_current_directory() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));

    env.command_in(&repo)
        .args(["proof", "--since", "main"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains(format!("repo: {}", repo.canonicalize().unwrap().display()))
                .and(predicate::str::contains("branch: main"))
                .and(predicate::str::contains("dirty: no"))
                .and(predicate::str::contains(
                    "choose a base ref with changed files, then rerun proof",
                )),
        );
}

#[test]
fn proof_fails_outside_git_repo() {
    let env = CliEnv::new();

    env.command_in(env.home.path())
        .args(["proof", "--since", "main"])
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("not a git repository")
                .and(predicate::str::contains("--repo <PATH>")),
        );
}

#[test]
fn proof_fails_on_invalid_since_ref() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));

    env.command_in(&repo)
        .args(["proof", "--since", "missing-ref"])
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("invalid git base ref")
                .and(predicate::str::contains("missing-ref")),
        );
}

#[test]
fn proof_reports_changed_files_and_diff_stats() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::write(repo.join("README.md"), "# test\n\nchanged\n").unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "README.md", "src/main.rs"]);
    git(&repo, ["commit", "-m", "change files"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Changed:")
                .and(predicate::str::contains("files: 2"))
                .and(predicate::str::contains("additions: 3"))
                .and(predicate::str::contains("deletions: 0"))
                .and(predicate::str::contains("docs only: no"))
                .and(predicate::str::contains("M README.md (+2 -0)"))
                .and(predicate::str::contains("A src/main.rs (+1 -0)")),
        );
}

#[test]
fn proof_reports_docs_only_changes() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::write(repo.join("docs").join("cli.md"), "docs\n").unwrap();
    git(&repo, ["add", "docs/cli.md"]);
    git(&repo, ["commit", "-m", "docs"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Changed:")
                .and(predicate::str::contains("files: 1"))
                .and(predicate::str::contains("docs only: yes"))
                .and(predicate::str::contains("A docs/cli.md (+1 -0)")),
        );
}

#[test]
fn proof_reports_risky_changed_paths() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join(".github").join("workflows")).unwrap();
    fs::create_dir_all(repo.join("k8s")).unwrap();
    fs::create_dir_all(repo.join("terraform")).unwrap();
    fs::create_dir_all(repo.join("db").join("migrations")).unwrap();
    fs::create_dir_all(repo.join("src").join("auth")).unwrap();
    fs::write(repo.join(".github/workflows/ci.yml"), "ci\n").unwrap();
    fs::write(repo.join("k8s/deployment.yaml"), "deployment\n").unwrap();
    fs::write(repo.join("terraform/main.tf"), "resource\n").unwrap();
    fs::write(repo.join("db/migrations/001.sql"), "select 1;\n").unwrap();
    fs::write(
        repo.join("src/auth/config.rs"),
        "pub const X: &str = \"x\";\n",
    )
    .unwrap();
    git(
        &repo,
        [
            "add",
            ".github/workflows/ci.yml",
            "k8s/deployment.yaml",
            "terraform/main.tf",
            "db/migrations/001.sql",
            "src/auth/config.rs",
        ],
    );
    git(&repo, ["commit", "-m", "risky paths"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Risk:")
                .and(predicate::str::contains("risk level: elevated"))
                .and(predicate::str::contains("risky files: 5"))
                .and(predicate::str::contains("CI/CD: .github/workflows/ci.yml"))
                .and(predicate::str::contains("Kubernetes: k8s/deployment.yaml"))
                .and(predicate::str::contains("Terraform: terraform/main.tf"))
                .and(predicate::str::contains("database: db/migrations/001.sql"))
                .and(predicate::str::contains("auth: src/auth/config.rs"))
                .and(predicate::str::contains("config: src/auth/config.rs")),
        );
}

#[test]
fn proof_reports_docs_only_changes_as_low_risk() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::write(repo.join("docs").join("security.md"), "security docs\n").unwrap();
    git(&repo, ["add", "docs/security.md"]);
    git(&repo, ["commit", "-m", "docs"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Risk:")
                .and(predicate::str::contains("risk level: low"))
                .and(predicate::str::contains("risky files: 0"))
                .and(predicate::str::contains("elevated").not()),
        );
}

#[test]
fn proof_reports_multiple_risk_categories_for_one_path() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("infra").join("prod")).unwrap();
    fs::write(repo.join("infra/prod/secrets.tf"), "resource\n").unwrap();
    git(&repo, ["add", "infra/prod/secrets.tf"]);
    git(&repo, ["commit", "-m", "infra secret"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Risk:")
                .and(predicate::str::contains("risky files: 1"))
                .and(predicate::str::contains("secrets: infra/prod/secrets.tf"))
                .and(predicate::str::contains("infra: infra/prod/secrets.tf"))
                .and(predicate::str::contains(
                    "production: infra/prod/secrets.tf",
                ))
                .and(predicate::str::contains("Terraform: infra/prod/secrets.tf")),
        );
}

#[test]
fn proof_reports_deleted_and_renamed_files() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::write(repo.join("delete-me.txt"), "delete\n").unwrap();
    fs::write(repo.join("old-name.txt"), "rename\n").unwrap();
    git(&repo, ["add", "delete-me.txt", "old-name.txt"]);
    git(&repo, ["commit", "-m", "add files"]);
    fs::remove_file(repo.join("delete-me.txt")).unwrap();
    git(&repo, ["mv", "old-name.txt", "new-name.txt"]);
    git(&repo, ["commit", "-am", "delete and rename"]);

    env.command_in(&repo)
        .args(["proof", "--since", "main~1"])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("files: 2")
                .and(predicate::str::contains("D delete-me.txt (+0 -1)"))
                .and(predicate::str::contains(
                    "R old-name.txt -> new-name.txt (+0 -0)",
                )),
        );
}

#[test]
fn proof_correlates_sessions_to_current_repo() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("repo-session.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T14:00:00Z\",\"session_id\":\"session-repo\",\"workspace_path\":\"{}\",\"title\":\"Repo work\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T14:00:05Z\",\"session_id\":\"session-repo\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0}}}}\n\
             {{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T14:00:10Z\",\"session_id\":\"session-repo\",\"file_change\":{{\"path\":\"src/main.rs\",\"change_type\":\"modified\",\"lines_added\":1,\"lines_deleted\":0}}}}\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();
    fs::write(
        codex_root.join("other-session.jsonl"),
        "{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T15:00:00Z\",\"session_id\":\"session-other\",\"workspace_path\":\"/tmp/other\",\"title\":\"Other work\"}\n",
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Codex:")
                .and(predicate::str::contains("relevant sessions: 1"))
                .and(predicate::str::contains("ambiguous sessions: 0"))
                .and(predicate::str::contains(
                    "session-repo Repo work [workspace, command-cwd, file-change]",
                ))
                .and(predicate::str::contains("session-other").not()),
        );
}

#[test]
fn proof_reports_ambiguous_session_overlap() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("ambiguous-session.jsonl"),
        "{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T14:00:10Z\",\"session_id\":\"session-ambiguous\",\"file_change\":{\"path\":\"main.rs\",\"change_type\":\"modified\",\"lines_added\":1,\"lines_deleted\":0}}\n",
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("relevant sessions: 0")
                .and(predicate::str::contains("ambiguous sessions: 1"))
                .and(predicate::str::contains(
                    "session-ambiguous (untitled) [ambiguous-file-name]",
                )),
        );
}

#[test]
fn proof_reports_risky_commands_from_relevant_sessions() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("risky-session.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T14:00:00Z\",\"session_id\":\"session-risk\",\"workspace_path\":\"{}\",\"title\":\"Risky deploy\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T14:00:05Z\",\"session_id\":\"session-risk\",\"command\":{{\"cmd\":\"kubectl apply -f k8s/prod.yaml\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"customer secret output should not print\"}}}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T14:00:06Z\",\"session_id\":\"session-risk\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"ok\"}}}}\n",
            repo_root.display(),
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Risky commands:")
                .and(predicate::str::contains("relevant: 1"))
                .and(predicate::str::contains("ambiguous: 0"))
                .and(predicate::str::contains(
                    "high kubectl session-risk Risky deploy: kubectl apply -f k8s/prod.yaml",
                ))
                .and(predicate::str::contains(
                    "reason: production/destructive arguments",
                ))
                .and(predicate::str::contains("elevated cargo").not())
                .and(predicate::str::contains("customer secret output should not print").not()),
        );
}

#[test]
fn proof_reports_risky_commands_from_ambiguous_sessions_separately() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("ambiguous-risk.jsonl"),
        concat!(
            "{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T14:00:10Z\",\"session_id\":\"session-ambiguous-risk\",\"file_change\":{\"path\":\"main.rs\",\"change_type\":\"modified\",\"lines_added\":1,\"lines_deleted\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T14:00:20Z\",\"session_id\":\"session-ambiguous-risk\",\"command\":{\"cmd\":\"aws s3 rm s3://prod-bucket/file\",\"status\":\"success\",\"exit_code\":0}}\n"
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Risky commands:")
                .and(predicate::str::contains("relevant: 0"))
                .and(predicate::str::contains("ambiguous: 1"))
                .and(predicate::str::contains(
                    "ambiguous high aws session-ambiguous-risk (untitled): aws s3 rm s3://prod-bucket/file",
                )),
        );
}

#[test]
fn proof_excludes_unrelated_risky_commands() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("unrelated-risk.jsonl"),
        "{\"type\":\"command\",\"timestamp\":\"2026-05-18T14:00:20Z\",\"session_id\":\"session-other-risk\",\"command\":{\"cmd\":\"rm -rf /tmp/prod\",\"cwd\":\"/tmp/other\",\"status\":\"success\",\"exit_code\":0}}\n",
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Risky commands:")
                .and(predicate::str::contains("relevant: 0"))
                .and(predicate::str::contains("ambiguous: 0"))
                .and(predicate::str::contains("rm -rf").not()),
        );
}

#[test]
fn proof_decision_ready_with_relevant_passing_verification() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("ready.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T15:00:00Z\",\"session_id\":\"session-ready\",\"workspace_path\":\"{}\",\"title\":\"Ready evidence\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:00:05Z\",\"session_id\":\"session-ready\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"SECRET_TOKEN should not print\"}}}}\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: READY"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "review and paste the report where proof is needed",
                ))
                .and(predicate::str::contains(
                    "reason: relevant verification passed: session-ready cargo test",
                ))
                .and(predicate::str::contains("SECRET_TOKEN").not()),
        );
}

#[test]
fn proof_decision_ready_with_resolved_fail_then_pass() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("resolved.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T15:10:00Z\",\"session_id\":\"session-resolved\",\"workspace_path\":\"{}\",\"title\":\"Resolved evidence\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:10:05Z\",\"session_id\":\"session-resolved\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"failed\",\"exit_code\":101,\"output\":\"failure details should not print\"}}}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:10:30Z\",\"session_id\":\"session-resolved\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"success\",\"exit_code\":0,\"output\":\"ok\"}}}}\n",
            repo_root.display(),
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: READY"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "review and paste the report where proof is needed",
                ))
                .and(predicate::str::contains(
                    "reason: resolved verification failure: session-resolved cargo test",
                ))
                .and(predicate::str::contains("failure details should not print").not()),
        );
}

#[test]
fn proof_decision_not_ready_with_unresolved_relevant_failure() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("not-ready.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T15:20:00Z\",\"session_id\":\"session-not-ready\",\"workspace_path\":\"{}\",\"title\":\"Broken evidence\"}}\n\
             {{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:20:05Z\",\"session_id\":\"session-not-ready\",\"command\":{{\"cmd\":\"cargo test\",\"cwd\":\"{}\",\"status\":\"failed\",\"exit_code\":101,\"output\":\"private failure should not print\"}}}}\n",
            repo_root.display(),
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: NOT READY"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "resolve the listed verification failures and rerun proof",
                ))
                .and(predicate::str::contains(
                    "reason: unresolved verification failure: session-not-ready cargo test",
                ))
                .and(predicate::str::contains("private failure should not print").not()),
        );
}

#[test]
fn proof_decision_unknown_when_db_is_missing() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::write(repo.join("README.md"), "# changed\n").unwrap();
    git(&repo, ["add", "README.md"]);
    git(&repo, ["commit", "-m", "change docs"]);
    let missing_db = env.home.path().join("missing.db");

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            missing_db.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: UNKNOWN"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "run `prooflog init` and `prooflog ingest --codex`, then rerun proof",
                ))
                .and(predicate::str::contains(
                    "reason: local proof database is missing",
                )),
        );

    let output = env
        .command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--format",
            "json",
            "--db",
            missing_db.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["next_actions"][0],
        "run `prooflog init` and `prooflog ingest --codex`, then rerun proof"
    );
}

#[test]
fn proof_decision_unknown_without_relevant_sessions() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("unrelated.jsonl"),
        "{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:30:05Z\",\"session_id\":\"session-unrelated\",\"command\":{\"cmd\":\"cargo test\",\"cwd\":\"/tmp/other\",\"status\":\"success\",\"exit_code\":0}}\n",
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: UNKNOWN"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "ingest Codex history for this repository, then rerun proof",
                ))
                .and(predicate::str::contains(
                    "reason: no relevant Codex sessions",
                )),
        );
}

#[test]
fn proof_decision_unknown_without_verification_evidence() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let repo_root = repo.canonicalize().unwrap();
    fs::write(
        codex_root.join("no-verification.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"timestamp\":\"2026-05-18T15:40:00Z\",\"session_id\":\"session-no-verification\",\"workspace_path\":\"{}\",\"title\":\"No verification\"}}\n",
            repo_root.display()
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: UNKNOWN"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "run verification commands for this change, ingest Codex history, then rerun proof",
                ))
                .and(predicate::str::contains(
                    "reason: no relevant verification evidence",
                )),
        );
}

#[test]
fn proof_decision_unknown_with_ambiguous_only_evidence() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    git(&repo, ["add", "src/main.rs"]);
    git(&repo, ["commit", "-m", "add code"]);
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("ambiguous-decision.jsonl"),
        concat!(
            "{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T15:50:00Z\",\"session_id\":\"session-ambiguous-decision\",\"file_change\":{\"path\":\"main.rs\",\"change_type\":\"modified\",\"lines_added\":1,\"lines_deleted\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T15:50:10Z\",\"session_id\":\"session-ambiguous-decision\",\"command\":{\"cmd\":\"cargo test\",\"status\":\"success\",\"exit_code\":0}}\n"
        ),
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main~1",
            "--db",
            env.db_file().to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Decision:")
                .and(predicate::str::contains("status: UNKNOWN"))
                .and(predicate::str::contains("Why:"))
                .and(predicate::str::contains("Next:"))
                .and(predicate::str::contains(
                    "rerun verification from this repository so evidence can be linked directly",
                ))
                .and(predicate::str::contains(
                    "reason: only ambiguous verification evidence found",
                )),
        );
}

#[test]
fn proof_handles_missing_db_for_session_correlation() {
    let env = CliEnv::new();
    let repo = init_git_repo(env.home.path().join("repo"));
    let missing_db = env.home.path().join("missing.db");

    env.command_in(&repo)
        .args([
            "proof",
            "--since",
            "main",
            "--db",
            missing_db.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains("Codex:")
                .and(predicate::str::contains("relevant sessions: 0"))
                .and(predicate::str::contains("ambiguous sessions: 0")),
        );
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
fn missing_home_and_userprofile_reports_actionable_error() {
    let mut cmd = Command::cargo_bin("prooflog").unwrap();
    cmd.env_clear()
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("home directory is not set"));
}

#[test]
fn default_paths_live_under_prooflog_home_directory() {
    let home = tempfile::tempdir().unwrap();
    let prooflog_home = home.path().join(".prooflog");
    let config_file = prooflog_home.join("config.toml");
    let db_file = prooflog_home.join("prooflog.db");

    let mut cmd = Command::cargo_bin("prooflog").unwrap();
    cmd.env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("ignored-config"))
        .env("XDG_DATA_HOME", home.path().join("ignored-data"))
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(config_file.display().to_string()));

    let config = fs::read_to_string(config_file).unwrap();
    assert!(config.contains(&format!("db_path = \"{}\"", db_file.display())));
}

#[test]
fn default_paths_fall_back_to_userprofile_when_home_is_missing() {
    let userprofile = tempfile::tempdir().unwrap();
    let prooflog_home = userprofile.path().join(".prooflog");
    let config_file = prooflog_home.join("config.toml");
    let db_file = prooflog_home.join("prooflog.db");

    let mut cmd = Command::cargo_bin("prooflog").unwrap();
    cmd.env_clear()
        .env("USERPROFILE", userprofile.path())
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
fn doctor_parser_reports_count_only_diagnostics() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("parser-diagnostics.jsonl"),
        "{\"type\":\"session_meta\",\"session_id\":\"session-parser\",\"workspace_path\":\"/workspace/prooflog\",\"title\":\"Parser diagnostics\"}\n\
         {\"type\":\"message\",\"session_id\":\"session-parser\",\"message\":{\"role\":\"user\",\"content\":\"SECRET_MESSAGE_TEXT\"}}\n\
         {\"type\":\"command\",\"session_id\":\"session-parser\",\"command\":{\"cmd\":\"cargo test -- SECRET_COMMAND_ARG\",\"cwd\":\"/workspace/prooflog\",\"status\":\"success\",\"exit_code\":0,\"output\":\"SECRET_COMMAND_OUTPUT\"}}\n\
         {\"type\":\"approval\",\"session_id\":\"session-parser\",\"approval\":{\"requested_action\":\"run\",\"decision\":\"approved\",\"command\":\"cargo test\"}}\n\
         {\"type\":\"file_change\",\"session_id\":\"session-parser\",\"file_change\":{\"path\":\"src/main.rs\",\"change_type\":\"modified\"}}\n\
         {\"unexpected\":true}\n\
         not json SECRET_PARSE_TEXT\n",
    )
    .unwrap();

    env.command()
        .args(["init", "--codex-root", codex_root.to_str().unwrap()])
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
        .success();

    env.command()
        .args(["doctor", "--parser"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Parser diagnostics:")
                .and(predicate::str::contains("raw events: 7"))
                .and(predicate::str::contains("malformed lines: 1"))
                .and(predicate::str::contains("unknown event shapes: 1"))
                .and(predicate::str::contains("sessions: 1"))
                .and(predicate::str::contains("messages: 1"))
                .and(predicate::str::contains("commands: 1"))
                .and(predicate::str::contains("approvals: 1"))
                .and(predicate::str::contains("file changes: 1"))
                .and(predicate::str::contains("proof facts: 1"))
                .and(predicate::str::contains(
                    "fixture reminder: add or update parser fixtures before changing parser behavior",
                ))
                .and(predicate::str::contains("SECRET_MESSAGE_TEXT").not())
                .and(predicate::str::contains("SECRET_COMMAND_ARG").not())
                .and(predicate::str::contains("SECRET_COMMAND_OUTPUT").not())
                .and(predicate::str::contains("SECRET_PARSE_TEXT").not())
                .and(predicate::str::contains("/workspace/prooflog").not()),
        );
}

#[test]
fn doctor_parser_missing_database_is_actionable() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();
    fs::remove_file(env.db_file()).unwrap();

    env.command()
        .args(["doctor", "--parser"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("run `prooflog init` and `prooflog ingest --codex`").and(
                predicate::str::contains(env.db_file().display().to_string()),
            ),
        );
    assert!(!env.db_file().exists());
}

#[test]
fn doctor_omits_parser_diagnostics_by_default() {
    let env = CliEnv::new();
    env.command().arg("init").assert().success();

    env.command()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Parser diagnostics:").not());
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
fn repeated_ingest_large_history_skips_unchanged_files_without_raw_line_work() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    for index in 0..75 {
        fs::write(
            codex_root.join(format!("session-{index:03}.jsonl")),
            format!("{{\"type\":\"session_meta\",\"session_id\":\"session-{index:03}\"}}\n"),
        )
        .unwrap();
    }

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
            predicate::str::contains("files discovered: 75")
                .and(predicate::str::contains("files ingested: 75"))
                .and(predicate::str::contains("files skipped: 0"))
                .and(predicate::str::contains("raw events stored: 75")),
        );

    assert_eq!(raw_event_rows(env.db_file()).len(), 75);

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
            predicate::str::contains("files discovered: 75")
                .and(predicate::str::contains("files ingested: 0"))
                .and(predicate::str::contains("files skipped: 75"))
                .and(predicate::str::contains("raw events stored: 0"))
                .and(predicate::str::contains("raw events skipped: 0")),
        );

    assert_eq!(raw_event_rows(env.db_file()).len(), 75);
}

#[test]
fn changed_jsonl_file_removes_stale_raw_rows_after_shrinking() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    let file = codex_root.join("session.jsonl");
    fs::write(
        &file,
        "{\"type\":\"session_meta\",\"session_id\":\"session-stale\"}\n{\"type\":\"message\",\"session_id\":\"session-stale\",\"message\":{\"role\":\"user\",\"content\":\"remove me\"}}\n",
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

    fs::write(
        &file,
        "{\"type\":\"session_meta\",\"session_id\":\"session-stale\"}\n",
    )
    .unwrap();
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
                .and(predicate::str::contains("raw events stored: 0"))
                .and(predicate::str::contains("raw events removed: 1")),
        );

    let rows = raw_event_rows(env.db_file());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].line_number, 1);
    assert_eq!(message_rows(env.db_file()).len(), 0);
    assert_eq!(raw_event_fts_match_count(env.db_file(), "remove"), 0);
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
fn ingest_derives_verification_proof_facts() {
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

    let facts = proof_fact_rows(env.db_file());
    assert_eq!(facts.len(), 1);
    assert!(facts[0].session_id.is_some());
    assert!(facts[0].command_id.is_some());
    assert_eq!(facts[0].kind, "verification");
    assert_eq!(facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(facts[0].status, "passed");
    assert!(facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("detector=cargo test")));
    assert!(facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("confidence=high")));
}

#[test]
fn verification_detector_records_failed_and_unknown_statuses() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"session_id\":\"session-a\",\"command\":{\"cmd\":\"cargo test\",\"status\":\"failure\",\"exit_code\":101}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:05Z\",\"session_id\":\"session-a\",\"command\":{\"cmd\":\"npm run build\"}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:10Z\",\"session_id\":\"session-a\",\"command\":{\"cmd\":\"aws s3 ls\",\"status\":\"success\",\"exit_code\":0}}\n"
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

    let facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "verification")
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(facts[0].status, "failed");
    assert_eq!(facts[1].subject.as_deref(), Some("npm run build"));
    assert_eq!(facts[1].status, "unknown");
}

#[test]
fn verification_detector_covers_supported_command_families() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"command\":{\"cmd\":\"go test ./...\",\"status\":\"success\",\"exit_code\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:01Z\",\"command\":{\"cmd\":\"pytest\",\"status\":\"success\",\"exit_code\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:02Z\",\"command\":{\"cmd\":\"pnpm lint\",\"status\":\"success\",\"exit_code\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:03Z\",\"command\":{\"cmd\":\"make build\",\"status\":\"success\",\"exit_code\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:04Z\",\"command\":{\"cmd\":\"tsc --noEmit\",\"status\":\"success\",\"exit_code\":0}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:05Z\",\"command\":{\"cmd\":\"eslint .\",\"status\":\"success\",\"exit_code\":0}}\n"
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

    let subjects = proof_fact_rows(env.db_file())
        .into_iter()
        .filter_map(|fact| fact.subject)
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        vec![
            "go test ./...",
            "pytest",
            "pnpm lint",
            "make build",
            "tsc --noEmit",
            "eslint ."
        ]
    );
}

#[test]
fn repeated_ingest_does_not_duplicate_proof_facts() {
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

    assert_eq!(proof_fact_rows(env.db_file()).len(), 4);
}

#[test]
fn ingest_derives_failure_proof_facts_from_unresolved_fixture() {
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

    let failure_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure")
        .collect::<Vec<_>>();
    assert_eq!(failure_facts.len(), 1);
    assert!(failure_facts[0].session_id.is_some());
    assert!(failure_facts[0].command_id.is_some());
    assert_eq!(failure_facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(failure_facts[0].status, "failed");
    assert!(failure_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("signal=exit-code")));
    assert!(failure_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("exit_code=101")));
}

#[test]
fn failure_detector_preserves_fail_then_pass_evidence() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("02_fail_then_pass.jsonl"),
        include_str!("fixtures/codex/02_fail_then_pass.jsonl"),
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

    let facts = proof_fact_rows(env.db_file());
    let verification_statuses = facts
        .iter()
        .filter(|fact| fact.kind == "verification")
        .map(|fact| fact.status.as_str())
        .collect::<Vec<_>>();
    let failure_facts = facts
        .iter()
        .filter(|fact| fact.kind == "failure")
        .collect::<Vec<_>>();

    assert_eq!(verification_statuses, vec!["failed", "passed"]);
    assert_eq!(failure_facts.len(), 1);
    assert_eq!(failure_facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(failure_facts[0].status, "failed");
}

#[test]
fn failure_detector_records_output_token_failures() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"command\":{\"cmd\":\"custom verify\",\"output\":\"permission denied while opening config\"}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:01Z\",\"command\":{\"cmd\":\"local check\",\"output\":\"command not found\"}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:02Z\",\"command\":{\"cmd\":\"cargo test\",\"status\":\"success\",\"exit_code\":0,\"output\":\"test result: ok\"}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:03Z\",\"command\":{\"cmd\":\"aws s3 ls\",\"status\":\"success\",\"exit_code\":0,\"output\":\"ok\"}}\n"
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

    let failure_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure")
        .collect::<Vec<_>>();
    assert_eq!(failure_facts.len(), 2);
    assert_eq!(failure_facts[0].subject.as_deref(), Some("custom verify"));
    assert!(failure_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("token=permission denied")));
    assert_eq!(failure_facts[1].subject.as_deref(), Some("local check"));
    assert!(failure_facts[1]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("token=command not found")));
}

#[test]
fn repeated_ingest_does_not_duplicate_failure_facts() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("03_unresolved_failure.jsonl"),
        include_str!("fixtures/codex/03_unresolved_failure.jsonl"),
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

    let failure_count = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure")
        .count();
    assert_eq!(failure_count, 1);
}

#[test]
fn failure_resolution_marks_fail_then_pass_resolved() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("02_fail_then_pass.jsonl"),
        include_str!("fixtures/codex/02_fail_then_pass.jsonl"),
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

    let resolution_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .collect::<Vec<_>>();
    assert_eq!(resolution_facts.len(), 1);
    assert_eq!(resolution_facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(resolution_facts[0].status, "resolved");
    assert!(resolution_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("resolution=exact-rerun")));
}

#[test]
fn failure_resolution_marks_unresolved_fixture_unresolved() {
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

    let resolution_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .collect::<Vec<_>>();
    assert_eq!(resolution_facts.len(), 1);
    assert_eq!(resolution_facts[0].subject.as_deref(), Some("cargo test"));
    assert_eq!(resolution_facts[0].status, "unresolved");
    assert!(resolution_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("resolution=no-later-pass")));
}

#[test]
fn failure_resolution_ignores_unrelated_later_passes() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"command\":{\"cmd\":\"cargo test\",\"status\":\"failure\",\"exit_code\":101}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:01Z\",\"command\":{\"cmd\":\"cargo build\",\"status\":\"success\",\"exit_code\":0}}\n"
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

    let resolution_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .collect::<Vec<_>>();
    assert_eq!(resolution_facts.len(), 1);
    assert_eq!(resolution_facts[0].status, "unresolved");
}

#[test]
fn failure_resolution_marks_ambiguous_same_detector_passes_unknown() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("commands.jsonl"),
        concat!(
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:00Z\",\"command\":{\"cmd\":\"cargo test tests/a.rs\",\"status\":\"failure\",\"exit_code\":101}}\n",
            "{\"type\":\"command\",\"timestamp\":\"2026-05-18T12:00:01Z\",\"command\":{\"cmd\":\"cargo test tests/b.rs\",\"status\":\"success\",\"exit_code\":0}}\n"
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

    let resolution_facts = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .collect::<Vec<_>>();
    assert_eq!(resolution_facts.len(), 1);
    assert_eq!(resolution_facts[0].status, "unknown");
    assert!(resolution_facts[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("resolution=ambiguous")));
}

#[test]
fn repeated_ingest_does_not_duplicate_failure_resolution_facts() {
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

    let resolution_count = proof_fact_rows(env.db_file())
        .into_iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .count();
    assert_eq!(resolution_count, 1);
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
fn ingest_derives_file_changes_from_raw_events() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("05_file_edits_diff.jsonl"),
        include_str!("fixtures/codex/05_file_edits_diff.jsonl"),
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

    let changes = file_change_rows(env.db_file());
    assert_eq!(changes.len(), 3);
    assert!(changes.iter().all(|change| change.raw_event_id > 0));
    assert!(changes.iter().all(|change| change.session_id.is_some()));
    assert_eq!(changes[0].path, ".github/workflows/ci.yml");
    assert_eq!(changes[0].change_type.as_deref(), Some("modified"));
    assert_eq!(changes[0].lines_added, Some(2));
    assert_eq!(changes[0].lines_deleted, Some(2));
    assert!(changes[0]
        .diff_text
        .as_deref()
        .is_some_and(|diff| diff.contains("cargo clippy --all-targets")));
    assert_eq!(changes[1].path, "docs/cli.md");
    assert_eq!(changes[1].lines_added, Some(2));
    assert_eq!(changes[1].lines_deleted, Some(0));
    assert_eq!(changes[2].path, "src/main.rs");
    assert_eq!(changes[2].lines_added, Some(3));
    assert_eq!(changes[2].lines_deleted, Some(1));
}

#[test]
fn file_change_derivation_allows_missing_optional_fields() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("file_changes.jsonl"),
        concat!(
            "{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T14:00:00Z\",\"file_change\":{\"path\":\"src/lib.rs\"}}\n",
            "{\"type\":\"file_change\",\"timestamp\":\"2026-05-18T14:00:10Z\",\"file_change\":{\"change_type\":\"modified\"}}\n",
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

    let changes = file_change_rows(env.db_file());
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "src/lib.rs");
    assert!(changes[0].session_id.is_none());
    assert!(changes[0].change_type.is_none());
    assert!(changes[0].diff_text.is_none());
    assert!(changes[0].lines_added.is_none());
    assert!(changes[0].lines_deleted.is_none());
}

#[test]
fn repeated_ingest_does_not_duplicate_file_changes() {
    let env = CliEnv::new();
    let codex_root = env.home.path().join("codex-history");
    fs::create_dir_all(&codex_root).unwrap();
    fs::write(
        codex_root.join("05_file_edits_diff.jsonl"),
        include_str!("fixtures/codex/05_file_edits_diff.jsonl"),
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

    assert_eq!(file_change_rows(env.db_file()).len(), 3);
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
            predicate::str::contains("files discovered: 2")
                .and(predicate::str::contains("warnings: 1"))
                .and(predicate::str::contains("Warnings:"))
                .and(predicate::str::contains("could not read")),
        );

    assert_eq!(codex_file_rows(env.db_file()).len(), 1);
}

struct CliEnv {
    home: TempDir,
}

impl CliEnv {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::cargo_bin("prooflog").unwrap();
        cmd.env_clear().env("HOME", self.home.path());
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
        self.home.path().join(".prooflog").join("config.toml")
    }

    fn db_file(&self) -> std::path::PathBuf {
        self.home.path().join(".prooflog").join("prooflog.db")
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

struct FileChangeRow {
    raw_event_id: i64,
    session_id: Option<i64>,
    path: String,
    change_type: Option<String>,
    diff_text: Option<String>,
    lines_added: Option<i64>,
    lines_deleted: Option<i64>,
}

struct ProofFactRow {
    session_id: Option<i64>,
    command_id: Option<i64>,
    kind: String,
    subject: Option<String>,
    status: String,
    reason: Option<String>,
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

fn file_change_rows(db_path: impl AsRef<std::path::Path>) -> Vec<FileChangeRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT raw_event_id, session_id, path, change_type, diff_text, lines_added, lines_deleted
             FROM file_changes
             ORDER BY path",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(FileChangeRow {
            raw_event_id: row.get(0)?,
            session_id: row.get(1)?,
            path: row.get(2)?,
            change_type: row.get(3)?,
            diff_text: row.get(4)?,
            lines_added: row.get(5)?,
            lines_deleted: row.get(6)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn proof_fact_rows(db_path: impl AsRef<std::path::Path>) -> Vec<ProofFactRow> {
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT session_id, command_id, kind, subject, status, reason
             FROM proof_facts
             ORDER BY id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(ProofFactRow {
            session_id: row.get(0)?,
            command_id: row.get(1)?,
            kind: row.get(2)?,
            subject: row.get(3)?,
            status: row.get(4)?,
            reason: row.get(5)?,
        })
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn normalize_report_output(output: &str, repo: impl AsRef<std::path::Path>) -> String {
    let repo = repo.as_ref().canonicalize().unwrap();
    let repo_display = repo.display().to_string();
    output
        .lines()
        .map(|line| {
            let line = line.replace(&repo_display, "/repo");
            if line.starts_with("| head |") {
                "| head | <head> |".to_string()
            } else if line.starts_with("| merge base |") {
                "| merge base | <merge-base> |".to_string()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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

fn init_git_repo(path: std::path::PathBuf) -> std::path::PathBuf {
    fs::create_dir_all(&path).unwrap();
    git(&path, ["init", "-b", "main"]);
    git(&path, ["config", "user.email", "test@example.com"]);
    git(&path, ["config", "user.name", "ProofLog Test"]);
    fs::write(path.join("README.md"), "# test\n").unwrap();
    git(&path, ["add", "README.md"]);
    git(&path, ["commit", "-m", "initial"]);
    path
}

fn git<const N: usize>(repo: &std::path::Path, args: [&str; N]) {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
