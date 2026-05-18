# Roadmap

This roadmap describes the intended implementation sequence for the public open-source project.

## 1. OSS Skeleton And Local Foundation

Goal: installable Rust CLI with config, database creation, doctor checks, and a clear README hero.

Work:

- Create Rust CLI skeleton.
- Implement config and local paths.
- Implement SQLite database initialization.
- Enforce owner-only DB permissions.
- Implement `prooflog doctor`.

## 2. Codex Raw Ingestion

Goal: discover Codex JSONL files, store every raw line safely, and tolerate unknown event shapes.

Work:

- Discover Codex JSONL files.
- Store every raw JSONL line.
- Add raw event FTS indexing.
- Implement ingest summary output.

## 3. Codex Parser Fixtures

Goal: five redacted real-world Codex fixtures define the parser contract.

Work:

- Create fixture `01_single_success.jsonl`.
- Create fixture `02_fail_then_pass.jsonl`.
- Create fixture `03_unresolved_failure.jsonl`.
- Create fixture `04_approval_risk.jsonl`.
- Create fixture `05_file_edits_diff.jsonl`.
- Add snapshot tests for all fixtures.

## 4. Derived Codex Extraction

Goal: derive sessions, messages, commands, approvals, file changes, and proof facts from raw events.

Work:

- Extract sessions from raw events.
- Extract messages.
- Extract shell commands and outputs.
- Extract approvals and sandbox context.
- Extract file changes and diffs.

## 5. Git Correlation And Proof Engine

Goal: correlate git changes with Codex evidence and classify verification status.

Work:

- Detect current git repo and branch.
- Compute changed files and diff stats.
- Correlate sessions to current repo.
- Implement verification detectors.
- Implement failure detector.
- Implement failure resolution logic.
- Implement risky path classifier.
- Implement risky command classifier.
- Implement decision engine.

## 6. Report UX And CLI Behavior

Goal: plain text, Markdown, JSON output, deterministic reports, and useful exit codes.

Work:

- Implement plain text proof report.
- Implement Markdown proof report.
- Implement exit codes.
- Add `--json` machine-readable output.

## 7. Privacy, Hardening, And Edge Cases

Goal: local privacy, redaction, malformed JSONL handling, large history support, and robust unknown states.

Work:

- Add redaction foundation.
- Handle malformed and partial JSONL robustly.
- Handle large Codex histories.
- Handle no evidence scenarios.
- Add parser diagnostics command.

## 8. Docs, Release, And Adoption Test

Goal: README, installation, contribution, release checklist, and seven-day continue-or-kill test.

Work:

- Write README hero.
- Write installation guide.
- Write contribution guide.
- Add release checklist.
- Run seven-day adoption test.
