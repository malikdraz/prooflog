# Contributing

ProofLog is a local-first CLI for proof reports around agent-assisted code changes. The core command is the product boundary:

```bash
prooflog proof --since main
```

Contributions should make that command more trustworthy, faster, clearer, safer, or easier to adopt.

## Project Focus

Good contributions improve one of these areas:

- Codex JSONL ingestion
- parser fixtures and parser robustness
- local SQLite storage safety
- git correlation
- verification, failure, and risk detection
- deterministic report output
- privacy and redaction
- CLI UX and actionable errors
- public documentation

## Non-Goals

Please do not open changes that move ProofLog toward:

- dashboard features
- cloud sync
- multi-agent orchestration
- semantic search
- embeddings
- launching or controlling Codex
- approving commands
- `AGENTS.md` generation
- broad observability or runbook tooling

Those ideas may be useful elsewhere, but they are outside ProofLog's current scope.

## Privacy Rules

ProofLog works with local session history, so privacy is part of correctness.

Do not commit or paste:

- real secrets, tokens, API keys, cookies, bearer tokens, or private keys
- customer names or private hostnames
- private repository names unless already public
- account IDs or cloud resource identifiers
- local usernames or home paths
- raw transcript excerpts that are not required for parser behavior
- command output containing private data

When adding fixtures, preserve parser-relevant structure while replacing private values with stable placeholders.

Good placeholders:

```text
session-redacted-001
/workspace/prooflog
example.invalid
REDACTED_TOKEN
customer-redacted
```

Do not redact JSON keys, event type names, timestamps, command status fields, exit codes, or parser-relevant nesting.

## Parser Fixture Rules

Parser behavior must be fixture-driven.

Before changing parser behavior:

1. Add or update a redacted fixture under `tests/fixtures/codex/`.
2. Keep one behavior theme per fixture when practical.
3. Preserve realistic event shape and ordering.
4. Add focused assertions in `tests/parser_fixtures.rs` when the behavior is important.
5. Update snapshots only after reviewing the diff.

Existing fixture docs live in [parser-fixtures.md](parser-fixtures.md).

## Snapshot Workflow

Run parser fixture tests first:

```bash
cargo test --test parser_fixtures
```

When an intentional parser change affects snapshots:

```bash
INSTA_UPDATE=always cargo test --test parser_fixtures
```

Review files under `tests/snapshots/` before committing. Snapshot churn without a parser behavior reason should be treated as a bug.

## Parser Contribution Checklist

For parser changes, include:

- redacted fixture coverage
- focused parser assertions
- snapshot updates when intentional
- malformed or unknown-shape behavior
- privacy review of fixture content
- proof report behavior if the change affects decisions
- docs updates when public behavior changes

Prefer `UNKNOWN` over false `READY` when evidence is incomplete or ambiguous.

## Report And CLI Checklist

For report or CLI changes, include:

- deterministic output ordering
- no raw transcript text by default
- no command output by default
- redaction for obvious secrets in user-facing output
- actionable errors with the failed operation and relevant identifier
- exit-code behavior when `prooflog proof` changes
- docs updates for new flags, formats, or decision behavior

## Validation Commands

Run the narrowest relevant check first, then the broader checks:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build
```

For docs-only changes, still run at least:

```bash
cargo test
cargo fmt --check
```

Before opening a PR, scan public files for accidental private planning metadata or secrets.

## Recommended Public Labels

Useful labels for issues and PRs:

- `area:cli`
- `area:codex-parser`
- `area:docs`
- `area:git`
- `area:privacy`
- `area:reports`
- `area:sqlite`
- `type:bug`
- `type:feature`
- `type:docs`
- `risk:privacy`
- `risk:regression`

Labels are for public triage only. Do not put private planning notes into public issues, PRs, commits, or docs.

## Commit And PR Notes

Keep commits factual and based on the diff. Do not include generated-by lines, co-author lines for tools, private issue IDs, or internal planning metadata.

In PR descriptions, include:

- what changed
- why it changes ProofLog behavior
- tests/checks run
- docs updated
- known limitations or deferred edge cases
