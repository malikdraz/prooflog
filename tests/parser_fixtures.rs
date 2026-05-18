use serde_json::Value;
use std::{fs, path::Path};

const FIXTURES: &[&str] = &[
    "01_single_success",
    "02_fail_then_pass",
    "03_unresolved_failure",
    "04_approval_risk",
    "05_file_edits_diff",
];

#[test]
fn parser_fixture_summaries_match_snapshots() {
    for fixture in FIXTURES {
        let summary = parse_fixture(format!("tests/fixtures/codex/{fixture}.jsonl"));
        insta::assert_snapshot!(*fixture, summary.snapshot());
    }
}

#[test]
fn fixture_01_single_success_has_expected_parser_contract() {
    let fixture = parse_fixture("tests/fixtures/codex/01_single_success.jsonl");

    assert_eq!(fixture.sessions, 1);
    assert!(fixture.user_messages >= 1);
    assert!(fixture.assistant_messages >= 1);
    assert!(fixture.commands >= 1);
    assert_eq!(fixture.pass_proof_facts, 1);
    assert_eq!(fixture.fail_proof_facts, 0);
}

#[test]
fn fixture_02_fail_then_pass_resolves_failed_verification() {
    let fixture = parse_fixture("tests/fixtures/codex/02_fail_then_pass.jsonl");

    assert_eq!(fixture.sessions, 1);
    assert_eq!(fixture.failed_verification_commands, 1);
    assert_eq!(fixture.passing_verification_commands, 1);
    assert_eq!(fixture.resolved_failures, 1);
    assert_eq!(fixture.unresolved_failures, 0);
}

#[test]
fn fixture_03_unresolved_failure_is_not_ready() {
    let fixture = parse_fixture("tests/fixtures/codex/03_unresolved_failure.jsonl");

    assert_eq!(fixture.sessions, 1);
    assert_eq!(fixture.failed_verification_commands, 1);
    assert_eq!(fixture.passing_verification_commands, 0);
    assert_eq!(fixture.resolved_failures, 0);
    assert_eq!(fixture.unresolved_failures, 1);
    assert_eq!(fixture.decision(), "NOT READY");
}

#[test]
fn fixture_04_approval_risk_covers_approval_and_risk_shapes() {
    let fixture = parse_fixture("tests/fixtures/codex/04_approval_risk.jsonl");

    assert_eq!(fixture.sessions, 1);
    assert!(fixture.approvals >= 1);
    assert_eq!(fixture.approved_actions, 1);
    assert_eq!(fixture.sandbox_approval_events, 1);
    assert!(fixture.risk_facts >= 1);
    assert!(fixture.friction_facts >= 1);
}

#[test]
fn fixture_05_file_edits_diff_covers_paths_stats_and_risk() {
    let fixture = parse_fixture("tests/fixtures/codex/05_file_edits_diff.jsonl");

    assert_eq!(fixture.sessions, 1);
    assert_eq!(fixture.file_changes, 3);
    assert_eq!(fixture.total_lines_added, 7);
    assert_eq!(fixture.total_lines_deleted, 3);
    assert_eq!(fixture.risky_path_facts, 1);
    assert_eq!(fixture.docs_low_risk_paths, 1);
    assert!(fixture.changed_paths.contains(&"src/main.rs".to_string()));
    assert!(fixture.changed_paths.contains(&"docs/cli.md".to_string()));
    assert!(fixture
        .changed_paths
        .contains(&".github/workflows/ci.yml".to_string()));
}

#[derive(Default)]
struct FixtureSummary {
    sessions: usize,
    user_messages: usize,
    assistant_messages: usize,
    commands: usize,
    pass_proof_facts: usize,
    fail_proof_facts: usize,
    passing_verification_commands: usize,
    failed_verification_commands: usize,
    resolved_failures: usize,
    unresolved_failures: usize,
    open_failures: Vec<String>,
    approvals: usize,
    approved_actions: usize,
    sandbox_approval_events: usize,
    risk_facts: usize,
    friction_facts: usize,
    file_changes: usize,
    total_lines_added: i64,
    total_lines_deleted: i64,
    risky_path_facts: usize,
    docs_low_risk_paths: usize,
    changed_paths: Vec<String>,
}

impl FixtureSummary {
    fn decision(&self) -> &'static str {
        if self.unresolved_failures > 0 {
            "NOT READY"
        } else if self.passing_verification_commands > 0 {
            "READY"
        } else {
            "UNKNOWN"
        }
    }

    fn snapshot(&self) -> String {
        let mut changed_paths = self.changed_paths.clone();
        changed_paths.sort();
        format!(
            "\
sessions: {sessions}
user_messages: {user_messages}
assistant_messages: {assistant_messages}
commands: {commands}
passing_verification_commands: {passing_verification_commands}
failed_verification_commands: {failed_verification_commands}
pass_proof_facts: {pass_proof_facts}
fail_proof_facts: {fail_proof_facts}
resolved_failures: {resolved_failures}
unresolved_failures: {unresolved_failures}
approvals: {approvals}
approved_actions: {approved_actions}
sandbox_approval_events: {sandbox_approval_events}
risk_facts: {risk_facts}
friction_facts: {friction_facts}
file_changes: {file_changes}
total_lines_added: {total_lines_added}
total_lines_deleted: {total_lines_deleted}
risky_path_facts: {risky_path_facts}
docs_low_risk_paths: {docs_low_risk_paths}
decision: {decision}
changed_paths: {changed_paths:?}
",
            sessions = self.sessions,
            user_messages = self.user_messages,
            assistant_messages = self.assistant_messages,
            commands = self.commands,
            passing_verification_commands = self.passing_verification_commands,
            failed_verification_commands = self.failed_verification_commands,
            pass_proof_facts = self.pass_proof_facts,
            fail_proof_facts = self.fail_proof_facts,
            resolved_failures = self.resolved_failures,
            unresolved_failures = self.unresolved_failures,
            approvals = self.approvals,
            approved_actions = self.approved_actions,
            sandbox_approval_events = self.sandbox_approval_events,
            risk_facts = self.risk_facts,
            friction_facts = self.friction_facts,
            file_changes = self.file_changes,
            total_lines_added = self.total_lines_added,
            total_lines_deleted = self.total_lines_deleted,
            risky_path_facts = self.risky_path_facts,
            docs_low_risk_paths = self.docs_low_risk_paths,
            decision = self.decision(),
            changed_paths = changed_paths,
        )
    }
}

fn parse_fixture(path: impl AsRef<Path>) -> FixtureSummary {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read fixture {}: {error}", path.display());
    });
    let mut summary = FixtureSummary::default();

    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).unwrap_or_else(|error| {
            panic!(
                "fixture {} line {} is not valid JSON: {error}",
                path.display(),
                index + 1
            );
        });
        apply_fixture_event(&mut summary, &value);
    }

    summary
}

fn apply_fixture_event(summary: &mut FixtureSummary, value: &Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("session_meta") => summary.sessions += 1,
        Some("message") => match value
            .get("message")
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
        {
            Some("user") => summary.user_messages += 1,
            Some("assistant") => summary.assistant_messages += 1,
            _ => {}
        },
        Some("command") => {
            summary.commands += 1;
            if is_risky_command(value) {
                summary.risk_facts += 1;
            }
            if has_friction_signal(value) {
                summary.friction_facts += 1;
            }
            if !is_verification_command(value) {
                return;
            }

            let subject = verification_subject(value);
            if is_passing_command(value) {
                summary.passing_verification_commands += 1;
                summary.pass_proof_facts += 1;
                if let Some(position) = summary
                    .open_failures
                    .iter()
                    .position(|failure| failure == &subject)
                {
                    summary.open_failures.remove(position);
                    summary.resolved_failures += 1;
                }
            } else if is_failing_command(value) {
                summary.failed_verification_commands += 1;
                summary.fail_proof_facts += 1;
                summary.open_failures.push(subject);
                summary.unresolved_failures += 1;
            }
        }
        Some("approval") => {
            summary.approvals += 1;
            if value
                .get("approval")
                .and_then(|approval| approval.get("decision"))
                .and_then(Value::as_str)
                == Some("approved")
            {
                summary.approved_actions += 1;
            }
            if value
                .get("approval")
                .and_then(|approval| approval.get("sandbox_mode"))
                .and_then(Value::as_str)
                .is_some()
            {
                summary.sandbox_approval_events += 1;
            }
        }
        Some("file_change") => {
            summary.file_changes += 1;
            let path = value
                .get("file_change")
                .and_then(|file_change| file_change.get("path"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            summary.changed_paths.push(path.clone());
            summary.total_lines_added += value
                .get("file_change")
                .and_then(|file_change| file_change.get("lines_added"))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            summary.total_lines_deleted += value
                .get("file_change")
                .and_then(|file_change| file_change.get("lines_deleted"))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            if is_risky_path(&path) {
                summary.risky_path_facts += 1;
            }
            if is_docs_path(&path) {
                summary.docs_low_risk_paths += 1;
            }
        }
        _ => {}
    }
    summary.unresolved_failures = summary.open_failures.len();
}

fn is_passing_command(value: &Value) -> bool {
    value.pointer("/command/exit_code").and_then(Value::as_i64) == Some(0)
}

fn is_failing_command(value: &Value) -> bool {
    value
        .pointer("/command/exit_code")
        .and_then(Value::as_i64)
        .is_some_and(|exit_code| exit_code != 0)
}

fn is_verification_command(value: &Value) -> bool {
    let command = value
        .pointer("/command/cmd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    matches!(
        command,
        "cargo test" | "cargo build" | "cargo clippy" | "go test ./..." | "npm test"
    )
}

fn verification_subject(value: &Value) -> String {
    value
        .pointer("/command/cmd")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn is_risky_command(value: &Value) -> bool {
    let command = value
        .pointer("/command/cmd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let first = command.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "aws" | "kubectl" | "terraform" | "helm" | "docker" | "rm" | "gh"
    )
}

fn has_friction_signal(value: &Value) -> bool {
    let output = value
        .pointer("/command/output")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    output.contains("sandbox") || output.contains("network") || output.contains("permission")
}

fn is_risky_path(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.ends_with(".toml")
        || path.contains("/config/")
        || path.contains("terraform")
}

fn is_docs_path(path: &str) -> bool {
    path.starts_with("docs/") || path.ends_with(".md")
}
