# Changelog

All notable ProofLog changes are recorded here.

The format follows Keep a Changelog conventions, and release versions follow semantic versioning.

## [Unreleased]

- Keep this section for changes that have landed but are not yet released.
- Move entries into the next versioned section before tagging.

## [0.1.0] - 2026-05-19

### Added

- Local Rust CLI with `init`, `doctor`, `ingest`, and `proof` commands.
- Local config and SQLite storage under `~/.prooflog`, with owner-only config and database permissions on Unix-like systems.
- Codex JSONL discovery, incremental raw ingestion, malformed-line handling, and parser diagnostics.
- Derived sessions, messages, commands, approvals, file changes, verification facts, failure facts, and failure-resolution facts.
- Git context detection, changed-file reporting, session correlation, risky path classification, and risky command classification.
- Conservative proof decision engine with `READY`, `NOT READY`, and `UNKNOWN` outcomes.
- Plain text, Markdown, and experimental JSON proof reports.
- Decision-based proof exit codes.
- Obvious-secret redaction in proof report output.
- Public installation, CLI, contributing, parser fixture, and release documentation.
