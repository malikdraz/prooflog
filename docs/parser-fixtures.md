# Parser Fixtures

Create five redacted fixtures from real Codex JSONL.

## Fixtures

1. `01_single_success.jsonl`
   - one user request
   - one assistant response
   - one passing verification command
   - expected: one session, one command, one PASS proof fact
   - status: present at `tests/fixtures/codex/01_single_success.jsonl`
2. `02_fail_then_pass.jsonl`
   - command fails, later rerun passes
   - expected: original failure marked resolved
   - status: present at `tests/fixtures/codex/02_fail_then_pass.jsonl`
3. `03_unresolved_failure.jsonl`
   - failed test, lint, build, or typecheck with no later passing rerun
   - expected: NOT READY
   - status: present at `tests/fixtures/codex/03_unresolved_failure.jsonl`
4. `04_approval_risk.jsonl`
   - approvals, sandbox or network friction, risky command
   - expected: approval extraction and risk facts
   - status: present at `tests/fixtures/codex/04_approval_risk.jsonl`
5. `05_file_edits_diff.jsonl`
   - file edits and diffs across code, config, and docs
   - expected: changed paths and risky path categories
   - status: present at `tests/fixtures/codex/05_file_edits_diff.jsonl`

## Redaction Rules

Preserve structure. Redact:

- secrets
- tokens
- account IDs
- private repository names if needed
- local usernames
- private paths
- customer names
- production hostnames

Do not redact event type names, JSON keys, command shapes, timestamps, or parser-relevant nesting.

## Snapshot Tests

`cargo test` runs fixture summary snapshots for every fixture in `tests/fixtures/codex/`.

Snapshots live under `tests/snapshots/` and intentionally cover compact parser summaries rather than raw transcript text. They currently track session/message counts, command verification counts, approval/risk facts, file-change stats, fixture decisions, and changed paths.

When parser fixture behavior intentionally changes, update snapshots explicitly:

```bash
INSTA_UPDATE=always cargo test --test parser_fixtures
```

Review the snapshot diff before committing. Final proof report snapshots are planned for the report renderer work; they are not implemented yet.
