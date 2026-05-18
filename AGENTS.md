# Repository Instructions

## Scope

This repo is the ProofLog project. Implementation has started with a Rust CLI skeleton. Continue implementation only when the user asks for project work or a tracked task requires it.

## Project Context

- Core command: `prooflog proof --since main`
- Current binary commands: `init`, `doctor`, `ingest`, `proof`
- `prooflog init` currently creates local TOML config, initializes the SQLite schema, and normalizes config/DB files to owner-only permissions on Unix-like systems.
- `prooflog doctor` currently reads config, prints storage/Codex/git readiness status, and warns on missing Codex/git context or unsafe config/DB file permissions.
- `prooflog ingest --codex` currently discovers local `.jsonl` files, records file metadata, stores non-empty raw JSONL lines with parse errors when malformed, rebuilds raw/message/command-output FTS indexes, derives session/message/command/approval/file-change rows, and classifies supported verification, failure, and failure-resolution evidence into proof facts.
- `prooflog proof --since <REF>` currently emits plain text and Markdown reports with scope, changed files, Codex evidence, verification, failures, risks, a conservative READY/NOT READY/UNKNOWN decision, why, next steps, and decision-based exit codes. JSON output is still planned.
- Local docs under `docs/` define the public project direction.

Use the repo-local docs as the source of truth for public project direction unless the user gives newer requirements.

## Product Boundary

ProofLog is a local-first Rust + SQLite CLI that reads local Codex JSONL plus git state and emits proof reports for senior engineers.

In scope for MVP:

- Codex JSONL ingestion
- Raw-first storage
- Parser fixtures from real Codex traces
- Git correlation
- Verification, failure, and risk classification
- Plain text and Markdown reports
- Useful exit codes
- Local privacy and redaction

Out of scope before the adoption test:

- Dashboard
- Tauri UI
- Cloud sync
- Multi-agent support
- Semantic search
- Embeddings
- AGENTS.md generation
- Launching or controlling Codex

## Implementation Guardrails

- Prefer UNKNOWN over false READY.
- Preserve raw events; derived tables are disposable.
- Add or update fixtures before changing parser behavior.
- Keep storage local by default.
- Do not print secrets or raw transcript content in reports by default.
- Every feature must improve `prooflog proof --since main`.

## Documentation Workflow

For documentation tasks:

- Keep product requirements in `docs/prd.md`.
- Keep architectural decisions in `docs/architecture.md`.
- Keep milestone and issue sequencing in `docs/roadmap.md`.
- Keep risk handling in `docs/risks.md`.
- Keep user-facing CLI behavior in `docs/cli.md`.

## Done Criteria

For implementation work, a task is not done unless:

- Code compiles.
- Tests pass.
- Parser changes have fixture coverage.
- Output is deterministic.
- CLI behavior changes are documented.
- Privacy impact has been considered.
- The change does not expand into dashboard-only behavior.
