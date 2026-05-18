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
   - messages
   - commands
   - approvals
   - file changes
   - proof facts
6. Git correlation
   - repo root
   - branch
   - merge base
   - changed files
   - diff stats
   - risky path categories
7. Proof engine
   - verification detectors
   - failure detectors
   - unresolved failure resolution
   - risk classification
   - final decision
8. Report renderer
   - plain text
   - Markdown
   - deterministic output
   - useful exit codes

## Data Model Notes

The current MVP schema initializes these tables. `codex_files` is populated by discovery, `raw_events` is populated by raw ingestion, and `sessions` is derived during ingest. The remaining derived tables are populated by later extraction work.

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

`raw_events_fts` is rebuilt after raw ingest. Message and command-output FTS tables are initialized for later derived extraction.
