# ProofLog Documentation

This directory holds public project documentation for ProofLog.

Keep docs small and current. If a document does not help a user install, operate, contribute to, release, or understand ProofLog, prefer removing it or moving the detail into tests, CLI help, or code comments.

## User Docs

- [Installation](installation.md)
- [CLI behavior](cli.md)
- [Demo script](demo.md)

## Contributor Docs

- [Contributing](contributing.md)
- [Parser fixtures](parser-fixtures.md)
- [Release checklist](release-checklist.md)

## Design Docs

- [Product requirements](prd.md)
- [Architecture](architecture.md)
- [Roadmap](roadmap.md)
- [Risk register](risks.md)
- [Operating model](operating-model.md)

## Current Repository State

ProofLog currently includes the Rust CLI, local config, SQLite storage, doctor checks, Codex JSONL ingestion, parser diagnostics, git correlation, verification/failure/risk classification, report redaction, text/Markdown/experimental JSON proof reports, and decision-based exit codes.

## Decision Rule

If a proposed change does not make `prooflog proof --since main` more trustworthy, faster, clearer, or easier to adopt, defer it.
