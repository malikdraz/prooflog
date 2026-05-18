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

`prooflog ingest --codex` discovers local Codex `.jsonl` files, records file metadata, and stores non-empty raw JSONL lines in SQLite.

`prooflog proof --since main` is still an explicit placeholder. It does not inspect git state or produce proof reports yet.

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

`prooflog ingest --codex` will later derive sessions, messages, commands, approvals, file changes, and proof facts from stored raw lines.

`prooflog proof --since main` will produce the core proof report.

## Current Argument Contract

`prooflog proof` requires `--since <REF>`.

`prooflog ingest` requires `--codex`.

`prooflog init` and `prooflog doctor` support `--db <PATH>` and `--codex-root <PATH>` overrides.

These contracts are covered by integration tests so future implementations keep the initial UX stable.

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

The schema is raw-first. Current ingest populates `codex_files` and `raw_events`; later parser work will populate the derived tables.

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

Current ingest output includes file discovery counts, raw event stored/skipped counts, malformed-line count, and grouped warnings. Raw event FTS indexing and derived parser extraction are planned follow-up work.

## Permission Warnings

On Unix-like systems, ProofLog expects config and DB files to use mode `0600`.

If `prooflog doctor` finds broader permissions, it prints a warning with a `chmod 600 <path>` fix. Permission warnings do not currently make `doctor` fail.

## Doctor Readiness

`prooflog doctor` currently reports:

- config path and resolved config values
- SQLite open status, migration version, FTS5 availability, and journal mode
- Codex root state and recursive `.jsonl` file count
- current git repo root and branch when available
- warnings for missing Codex root, no JSONL files, missing git repo, and unsafe file permissions

Warnings are non-fatal. Critical config and database errors still return a non-zero exit code.
