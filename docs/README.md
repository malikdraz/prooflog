# ProofLog Documentation

This directory is the repo-local starting point for ProofLog context.

## Read Order

1. [Product requirements](prd.md)
2. [Architecture](architecture.md)
3. [Roadmap](roadmap.md)
4. [CLI behavior](cli.md)
5. [Parser fixtures](parser-fixtures.md)
6. [Risk register](risks.md)
7. [Operating model](operating-model.md)
8. [Demo script](demo.md)

## Current Repository State

The repository currently contains the initial Rust CLI, local config handling, SQLite schema initialization, owner-only config/DB file permissions on Unix-like systems, doctor readiness checks, Codex JSONL file discovery metadata, raw JSONL line storage, raw/message/command-output FTS indexing for diagnostics, and derived session/message/command/approval/file-change rows. Proof-fact extraction, git correlation, reports, and final exit-code behavior are still planned roadmap work.

## Decision Rule

If a proposed change does not make `prooflog proof --since main` more trustworthy, faster, clearer, or easier to adopt, defer it.
