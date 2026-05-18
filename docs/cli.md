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

`prooflog doctor` reads the config file and prints resolved paths plus storage status.

The current config stores:

- database path
- Codex root
- redaction defaults

`prooflog ingest --codex` and `prooflog proof --since main` are still explicit placeholders. They do not create databases, read Codex history, inspect git state, or perform network access.

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

`prooflog init` will later add owner-only permission enforcement around local storage.

`prooflog doctor` will later check Codex history, database permissions, and git context.

`prooflog ingest --codex` will discover and ingest local Codex JSONL history.

`prooflog proof --since main` will produce the core proof report.

## Current Argument Contract

`prooflog proof` requires `--since <REF>`.

`prooflog ingest` requires `--codex`.

`prooflog init` and `prooflog doctor` support `--db <PATH>` and `--codex-root <PATH>` overrides.

These contracts are covered by integration tests so future implementations keep the initial UX stable.

## SQLite Schema

The initialized DB records migration version `1` and creates these MVP tables:

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

The schema is raw-first. Later parser and ingestion work will populate it.
