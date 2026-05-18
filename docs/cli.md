# CLI Behavior

This page describes the current public command surface.

## Implemented Now

The `prooflog` binary exists and exposes these top-level commands:

```bash
prooflog init
prooflog doctor
prooflog ingest --codex
prooflog proof --since main
```

`prooflog init` creates a local TOML config file and initializes the local SQLite database schema.

On Unix-like systems, `prooflog init` sets the config and DB files to owner-readable/writable only.

`prooflog doctor` reads the config file and prints resolved paths, storage status, Codex root status, Codex JSONL count, and git repo status. On Unix-like systems, it warns when config or DB file permissions are broader than owner-only.

The current config stores:

- database path
- Codex root
- redaction defaults

`prooflog ingest --codex` discovers local Codex `.jsonl` files, records file metadata, stores non-empty raw JSONL lines in SQLite, rebuilds raw/message/command-output FTS indexes, and derives session/message/command/approval/file-change rows.

`prooflog proof --since main` detects the current git repository context and prints repo root, branch or detached HEAD label, current HEAD, merge base, dirty working tree status, changed files, diff stats, and docs-only status. It remains an explicit proof-report placeholder and does not produce the final proof report yet.

## Local Paths

When `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are set, ProofLog uses:

```text
$XDG_CONFIG_HOME/prooflog/config.toml
$XDG_DATA_HOME/prooflog/prooflog.db
```

When those variables are not set, ProofLog falls back to:

```text
$HOME/.config/prooflog/config.toml
$HOME/.local/share/prooflog/prooflog.db
```

The default Codex root is:

```text
$HOME/.codex
```

## Planned Behavior

`prooflog init` will later extend storage hardening around directories and SQLite sidecar files if needed.

`prooflog doctor` will later add deeper parser diagnostics and richer git edge-case handling.

`prooflog ingest --codex` will later derive proof facts from stored raw lines.

`prooflog proof --since main` will later produce the core proof report.

## Current Argument Contract

`prooflog proof` requires `--since <REF>` and supports `--repo <PATH>`.

`prooflog ingest` requires `--codex`.

`prooflog init` and `prooflog doctor` support `--db <PATH>` and `--codex-root <PATH>` overrides.

These contracts are covered by integration tests so future implementations keep the initial UX stable.

## Proof Git Context

`prooflog proof --since <REF>` currently resolves git context and changed-file stats before report generation.

It prints:

- repository root
- current branch, or a detached HEAD label
- current HEAD
- merge base for `--since <REF>`
- dirty working tree status
- changed file count
- total additions and deletions
- docs-only status
- per-file status, path, additions, and deletions

Use `--repo <PATH>` to inspect a repository other than the current working directory. Running outside a git repository or passing an invalid base ref fails with an actionable error. Codex evidence correlation, final proof reports, and final decision exit codes are planned follow-up work.

## SQLite Schema

The initialized DB records migration version `2` and creates these MVP tables:

- `schema_migrations`
- `codex_files`
- `sessions`
- `raw_events`
- `messages`
- `commands`
- `approvals`
- `file_changes`
- `proof_facts`

It also creates these FTS5 tables:

- `raw_events_fts`
- `messages_fts`
- `command_output_fts`

The schema is raw-first. Current ingest populates `codex_files`, `raw_events`, `sessions`, `messages`, `commands`, `approvals`, and `file_changes`; later parser work will populate the remaining derived tables.

## Codex Discovery

`prooflog ingest --codex --codex-root <path>` recursively discovers lowercase `.jsonl` files under the configured root.

For each discovered file, it records:

- path
- size
- modified time
- SHA-256 hash

Repeated ingest skips unchanged file metadata and updates changed file metadata in place. Symlinked directories are skipped to avoid loops.

## Raw Event Storage

For each discovered file, `prooflog ingest --codex` reads JSONL content line-by-line and stores one `raw_events` row for each non-empty physical line.

Each raw event row records:

- source file id
- line number
- raw line text without the line ending
- line SHA-256 hash
- event type when a known top-level string field is present
- event time when a known top-level string field is present
- parse error when the line is malformed JSON

Malformed JSON lines do not abort ingest. Unknown valid JSON shapes are preserved with NULL derived metadata. Empty lines are skipped and counted in ingest output.

Current ingest output includes:

- files discovered
- files ingested
- files skipped
- raw events stored
- raw events skipped
- malformed lines
- unknown event shapes
- warning count

Warning details are grouped under `Warnings:` only when present. After ingest, `raw_events_fts` is rebuilt from stored raw events, `messages_fts` is rebuilt from derived messages, and `command_output_fts` is rebuilt from derived commands for internal diagnostics. These are not user-facing search commands, and richer derived parser extraction remains planned follow-up work.

## Session Derivation

After storing raw events, ingest derives `sessions` rows from parseable events with a top-level `session_id`.

When available, each derived session records:

- Codex session id
- workspace path
- model
- title or summary
- first event timestamp
- latest event timestamp
- linked raw event count
- parse status

Missing optional metadata is stored as NULL. Raw events with a known session id are linked back to the derived `sessions.id`.

## Message Derivation

Ingest derives `messages` rows from parseable message events with known roles and non-empty text.

When available, each derived message records:

- raw event link
- session link
- role
- message text
- event timestamp

Unknown message shapes and empty message text are skipped instead of guessed. Message text is indexed in `messages_fts` for internal diagnostics, but ingest does not print raw message text by default.

## Command Derivation

Ingest derives `commands` rows from parseable command events with a known command string.

When available, each derived command records:

- raw event link
- session link
- command string
- cwd
- status
- exit code
- output text
- start and end timestamps

Unknown command shapes and missing command strings are skipped instead of guessed. Command/output text is indexed in `command_output_fts` for internal diagnostics, but ingest does not print command output by default.

## Approval Derivation

Ingest derives `approvals` rows from parseable approval events.

When available, each derived approval records:

- raw event link
- session link
- requested action
- decision
- sandbox mode
- command
- event timestamp

Missing approval fields are stored as NULL. Unknown approval shapes are skipped instead of guessed. Ingest does not print approval commands or raw transcript content by default.

## File-Change Derivation

Ingest derives `file_changes` rows from parseable file-change events with a known path.

When available, each derived file change records:

- raw event link
- session link
- path
- change type
- diff text
- lines added
- lines deleted

Missing optional file-change fields are stored as NULL. Missing paths and unknown file-change shapes are skipped instead of guessed. Ingest does not print raw diff text by default. Proof-fact classification, git correlation, and proof report behavior are planned follow-up work.

## Permission Warnings

On Unix-like systems, ProofLog expects config and DB files to use mode `0600`.

If `prooflog doctor` finds broader permissions, it prints a warning with a `chmod 600 <path>` fix. Permission warnings do not currently make `doctor` fail.

## Doctor Readiness

`prooflog doctor` currently reports:

- config path and resolved config values
- SQLite open status, migration version, FTS5 availability, and journal mode
- required FTS table availability
- Codex root state and recursive `.jsonl` file count
- current git repo root and branch when available
- warnings for missing Codex root, no JSONL files, missing git repo, and unsafe file permissions

Warnings are non-fatal. Critical config and database errors still return a non-zero exit code.
