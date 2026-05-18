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

The handlers are placeholders. They print explicit "not implemented yet" messages and do not create config files, create databases, read Codex history, inspect git state, or perform network access.

## Planned Behavior

`prooflog init` will create local config and storage.

`prooflog doctor` will check config, database, Codex history, SQLite capabilities, permissions, and git context.

`prooflog ingest --codex` will discover and ingest local Codex JSONL history.

`prooflog proof --since main` will produce the core proof report.

## Current Argument Contract

`prooflog proof` requires `--since <REF>`.

`prooflog ingest` requires `--codex`.

Both contracts are covered by integration tests so future implementations keep the initial UX stable.
