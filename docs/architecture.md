# Architecture

## Principle

Raw events are the source of truth. Derived tables are disposable.

## Components

1. CLI layer
   - `clap`
   - commands: `init`, `doctor`, `ingest`, `proof`
2. Storage layer
   - `rusqlite`
   - WAL mode
   - FTS5
   - owner-only config and DB file permissions on Unix-like systems
3. Codex discovery
   - configurable Codex root
   - recursive JSONL discovery
   - mtime and sha256 based incremental ingestion
   - symlinked directories skipped to avoid loops
4. Raw ingestion
   - stores every non-empty line
   - records malformed lines
   - preserves unknown event shapes
   - rebuilds raw event FTS for diagnostics
5. Derived extraction
   - sessions derived during ingest
   - messages derived during ingest
   - commands derived during ingest
   - approvals derived during ingest
   - file changes derived during ingest
   - verification proof facts derived during ingest
   - failure proof facts derived during ingest
   - failure-resolution proof facts derived during ingest
6. Git context and correlation
   - repo root, branch, HEAD, merge base, and dirty status detected by `prooflog proof`
   - changed files and diff stats detected by `prooflog proof`
   - sessions correlated to repo by workspace, command cwd, and file-change overlap
   - risky path categories detected from changed paths
   - risky command families detected from relevant and ambiguous Codex sessions
7. Proof engine
   - verification detectors
   - conservative READY/NOT READY/UNKNOWN decision
8. Report renderer
   - plain text report
   - Markdown report planned
   - deterministic output
   - useful exit codes planned

## Data Model Notes

The current MVP schema initializes these tables. `codex_files` is populated by discovery, `raw_events` is populated by raw ingestion, `sessions`/`messages`/`commands`/`approvals`/`file_changes` are derived during ingest, and supported verification, failure, and resolution evidence is classified into `proof_facts`. Git context, changed files, diff stats, risky path categories, risky command families, session-to-repo correlation, and the conservative decision are detected at proof-command runtime.

- `codex_files`
- `sessions`
- `raw_events`
- `messages`
- `commands`
- `approvals`
- `file_changes`
- `proof_facts`
- `schema_migrations`

The current MVP schema also initializes these FTS5 tables:

- `raw_events_fts`
- `messages_fts`
- `command_output_fts`

The FTS tables are rebuilt after ingest from stored raw events, derived messages, and derived command output for internal diagnostics.
