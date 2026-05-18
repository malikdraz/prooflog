use serde_json::Value;
use std::{fs, path::Path};

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
