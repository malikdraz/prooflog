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

`prooflog ingest --codex` discovers local Codex `.jsonl` files, records file metadata, stores non-empty raw JSONL lines in SQLite, rebuilds raw/message/command-output FTS indexes, derives session/message/command/approval/file-change rows, and classifies supported verification, failure, and failure-resolution evidence into local proof facts.

`prooflog proof --since main` detects the current git repository context and prints repo root, branch or detached HEAD label, current HEAD, merge base, dirty working tree status, changed files, diff stats, docs-only status, risky changed paths, risky commands from relevant/ambiguous sessions, relevant/ambiguous Codex sessions from local storage, and a conservative READY/NOT READY/UNKNOWN decision section. It remains an explicit proof-report placeholder and does not produce the final report format yet.

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

`prooflog ingest --codex` will later add additional proof facts as report needs harden.

`prooflog proof --since main` will later produce the final core proof report format and exit-code mapping.

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
- risky path level, count, categories, and reasons
- risky command counts, families, severity, and reasons
- conservative proof decision status and reasons

Use `--repo <PATH>` to inspect a repository other than the current working directory. Running outside a git repository or passing an invalid base ref fails with an actionable error.

## Risk Path Classification

`prooflog proof --since <REF>` prints a `Risk:` section based on changed file paths.

Current path categories are:

- auth
- identity
- security
- secrets
- config
- infra
- migration
- CI/CD
- production
- Kubernetes
- Terraform
- database
- release

Docs-only changes are reported as `risk level: low` with zero risky files. Mixed non-doc changes are `risk level: elevated` when any changed path matches a category. The classifier is deterministic and path-based; it does not inspect file contents or make a final readiness decision.

## Risky Command Classification

`prooflog proof --since <REF>` prints a `Risky commands:` section from commands in relevant and ambiguous Codex sessions.

Current command families are:

- `aws`
- `kubectl`
- `terraform`
- `helm`
- `docker`
- `gh`
- `rm`
- `chmod`
- `chown`
- `curl`
- `scp`
- `ssh`

Risky commands are reported with family, session, command subject, severity, and reason. Production-like or destructive arguments such as `prod`, `production`, `--force`, `delete`, `destroy`, `apply`, `rm -rf`, `chmod 777`, `terraform apply`, `kubectl delete`, and similar patterns raise severity to `high`. ProofLog reports these commands; it does not block them, execute them, or print command output.

## Codex Session Correlation

`prooflog proof --since <REF>` reads the local ProofLog database when available and prints relevant and ambiguous Codex session counts.

Strong relevant signals include:

- session workspace path matching the repo root
- command cwd inside the repo
- file-change paths overlapping changed files

Weak file-name-only overlap is reported as ambiguous rather than hidden. Missing or empty local storage reports zero relevant and ambiguous sessions without failing this placeholder proof flow. Final proof reports and final decision exit codes are planned follow-up work.

## Proof Decision

`prooflog proof --since <REF>` prints a `Decision:` section with `status: READY`, `status: NOT READY`, or `status: UNKNOWN`, plus one or more deterministic `reason:` lines.

Current decision rules are intentionally conservative:

- `READY` requires changed files, at least one relevant Codex session, at least one relevant passed verification fact, no unresolved relevant verification failures, and no ambiguous relevant failure-resolution facts.
- `NOT READY` means relevant evidence proves a verification failure remains unresolved.
- `UNKNOWN` covers missing local storage, no relevant sessions, no changed files, no relevant verification evidence, unknown-only verification evidence, ambiguous-only evidence, or ambiguous failure resolution.

Decision reasons may include session ids, verification command subjects, and status summaries. They do not print command output or raw transcript text. This decision section does not yet change process exit codes; exit-code behavior is planned separately.

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

The schema is raw-first. Current ingest populates `codex_files`, `raw_events`, `sessions`, `messages`, `commands`, `approvals`, `file_changes`, and verification/failure/resolution rows in `proof_facts`; later parser work will add risk facts.

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

## Verification Proof Facts

Ingest classifies supported verification commands from derived `commands` rows and stores them in `proof_facts` with `kind = 'verification'`.

Currently recognized command families include:

- `cargo test`, `cargo build`, `cargo clippy`
- `go test`, `go build`, `golangci-lint`
- `pytest`, `ruff`
- `npm test`, `npm run build`, `npm run lint`, `npm run typecheck`
- `pnpm test`, `pnpm build`, `pnpm lint`, `pnpm typecheck`
- `make test`, `make build`
- `tsc`, `eslint`

Each verification fact records the linked session, linked command, command subject, conservative status, and detector reason. Exit code `0` is `passed`; non-zero exit codes are `failed`; missing or ambiguous command outcomes are `unknown`. Unknown command families are not classified.

## Failure Proof Facts

Ingest also classifies explicit command failure evidence from derived `commands` rows and stores it in `proof_facts` with `kind = 'failure'`.

Failure signals include:

- non-zero exit code
- failure-like status such as `failure`, `failed`, `fail`, or `error`
- output tokens such as `error`, `failed`, `permission denied`, `timed out`, `no such file`, `command not found`, `sandbox`, or `network`

Each failure fact records the linked session, linked command, command subject, `failed` status, and a deterministic reason that names the strongest signal. Exit code `0` and success-like statuses are treated as non-failure for this detector.

## Failure Resolution Proof Facts

Ingest derives `failure_resolution` proof facts for failed verification commands.

Resolution status is:

- `resolved` when a later passing verification command is an exact rerun or a compatible broader rerun of the same detector
- `unresolved` when no later passing verification command of the same detector exists
- `unknown` when later passing evidence exists but compatibility is ambiguous

Resolution facts are linked to the failed command and preserve a deterministic reason. Passing commands from unrelated verification detectors do not resolve failures. The proof decision engine consumes these facts conservatively and prefers UNKNOWN over false READY when evidence is incomplete.

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

Missing optional file-change fields are stored as NULL. Missing paths and unknown file-change shapes are skipped instead of guessed. Ingest does not print raw diff text by default. Final decisions and proof report behavior are planned follow-up work.

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
