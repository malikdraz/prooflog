# ProofLog

ProofLog is a local-first Rust + SQLite CLI that turns local Codex JSONL history and git state into a proof report for agent-assisted code changes.

```bash
prooflog proof --since main
```

## Promise

ProofLog gives a senior engineer a deterministic, local answer to one question: is this agent-assisted change proven enough to review, merge, or hand off?

## Before And After

Before ProofLog, the evidence is scattered across terminal output, Codex transcripts, git diffs, approvals, and notes.

After ProofLog, the evidence is summarized in one PR-pasteable report:

- what changed
- which local Codex sessions look relevant
- which verification commands passed
- which failures remain unresolved
- which files or commands look risky
- whether the change is `READY`, `NOT READY`, or `UNKNOWN`

No cloud. No dashboard. No agent orchestration. No transcript browsing.

## Install From Source

ProofLog is currently installed from this repository:

```bash
git clone https://github.com/malikdraz/prooflog.git
cd prooflog
cargo install --path .
```

Requires a stable Rust toolchain and local access to the Codex JSONL history you want to inspect.

## Quickstart

```bash
prooflog init
prooflog doctor
prooflog ingest --codex --codex-root ~/.codex
prooflog proof --since main
```

Markdown and JSON outputs are also available:

```bash
prooflog proof --since main --format md > prooflog.md
prooflog proof --since main --format json > prooflog.json
```

## Example Output

```text
PROOFLOG REPORT

Scope:
  repo: /home/user/src/example-project
  branch: feature/example-change
  since: main
  dirty: no

Changed:
  files: 18
  additions: 240
  deletions: 41
  docs only: no

Codex evidence:
Codex:
  relevant sessions: 3
  ambiguous sessions: 0

Verification:
  facts: 3
  passed: 2
  failed: 1
  unknown: 0

Failures:
  failure resolutions: 1
  unresolved: 1
  resolved: 0
  ambiguous: 0

Risks:
Risk:
  risk level: elevated
  risky files: 1

Decision:
  status: NOT READY
  reason: unresolved verification failure: session-a npm run lint

Next:
  resolve the listed verification failures and rerun proof
```

## Commands

```bash
prooflog init
prooflog doctor
prooflog doctor --parser
prooflog ingest --codex
prooflog proof --since <REF>
```

- `init` creates local config and initializes the SQLite database.
- `doctor` checks local config, storage, Codex history, git context, and file permissions.
- `doctor --parser` prints count-only parser diagnostics from local storage.
- `ingest --codex` discovers local `.jsonl` history, preserves raw events, derives parser tables, and classifies verification/failure evidence.
- `proof --since <REF>` correlates local proof evidence with git changes and emits text, Markdown, or JSON reports.

## Exit Codes

`prooflog proof --since <REF>` returns:

- `0` for `READY`
- `1` for `NOT READY`
- `2` for `UNKNOWN`
- `3` for runtime ProofLog errors

Invalid argument errors use the CLI parser's standard non-zero behavior.

## Current Status

Implemented now:

- local config and SQLite initialization
- owner-only config/DB permissions on Unix-like systems
- `doctor` readiness checks
- count-only parser diagnostics
- Codex JSONL discovery and incremental raw ingestion
- malformed-line and unknown-shape handling
- raw/message/command-output FTS indexes for diagnostics
- derived sessions, messages, commands, approvals, file changes, and proof facts
- git context, changed-file detection, and session correlation
- verification, failure, failure-resolution, risky-path, and risky-command classification
- conservative `READY` / `NOT READY` / `UNKNOWN` decision engine
- text, Markdown, and experimental JSON reports
- obvious-secret redaction in report output

## Non-Goals

ProofLog is not:

- a dashboard
- a cloud sync service
- a multi-agent framework
- a semantic search tool
- an embeddings store
- a Codex launcher
- an approval controller
- an `AGENTS.md` generator
- a full observability platform

## Project Docs

- [Documentation index](docs/README.md)
- [CLI behavior](docs/cli.md)
- [Product requirements](docs/prd.md)
- [Architecture notes](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Operating model](docs/operating-model.md)

## Design Principle

Every feature must improve:

```bash
prooflog proof --since main
```

If a feature does not make that command more trustworthy, faster, clearer, or easier to adopt, defer it.
