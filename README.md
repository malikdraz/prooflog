# ProofLog

ProofLog is a local-first Rust + SQLite CLI for turning local Codex JSONL session history and git state into a PR-ready proof report.

The core command is:

```bash
prooflog proof --since main
```

It should answer:

- What changed?
- Which Codex sessions appear relevant?
- Which verification commands passed?
- Which failures remain unresolved?
- Did the agent touch risky areas such as auth, config, infra, secrets, migrations, cloud, or production paths?
- Is this agent-assisted change ready to review, not ready, or unknown?

## Product Promise

`prooflog` reads local Codex JSONL and git state, then tells a senior engineer whether agent work is proven enough to review, merge, or hand off.

No cloud. No SDK. No dashboard. No agent orchestration.

## MVP Boundary

In scope:

- Codex-only MVP
- Local SQLite database
- Raw JSONL ingestion
- Parser fixtures from real Codex traces
- Git diff correlation
- Command extraction
- Verification, failure, and risk classification
- Plain text and Markdown proof reports
- Useful exit codes for shell, CI, and pre-PR workflows
- Local privacy and redaction foundations

Out of scope for MVP:

- Multi-agent support
- Tauri UI
- Embeddings
- Semantic summaries
- AGENTS.md generation
- Launching Codex
- Approving commands
- Cloud sync
- Team dashboards
- Full observability platform

## Success Criteria

Within 7 days of first usable release:

- Maintainer runs `prooflog proof --since main` before at least 5 agent-assisted commits or PRs.
- ProofLog catches at least one real verification gap.
- It replaces at least two manual transcript archaeology sessions.
- Install-to-first-report is under 5 minutes.
- At least two senior engineers can run it on their own Codex history without pairing.
- At least one external user asks for parser support, detector tuning, or report formatting.
- The project remains focused on proof reports, not browsing old sessions.

## Current Status

ProofLog currently has an initial Rust CLI with local config path handling, SQLite schema initialization, owner-only file permissions on Unix-like systems, first-time `doctor` readiness output, Codex JSONL file discovery, raw JSONL line storage, and raw event FTS indexing for diagnostics. The binary builds and exposes the planned MVP command surface:

```bash
prooflog --help
prooflog init
prooflog doctor
prooflog ingest --codex
prooflog proof --since main
```

`prooflog init` creates a local TOML config file, initializes the local SQLite database schema, and sets config/DB files to owner-readable/writable on Unix-like systems. `prooflog doctor` can read config, show storage status, count local Codex JSONL files, detect the current git repo when available, and warn on missing Codex/git context or broad config/DB file permissions. `prooflog ingest --codex` discovers `.jsonl` files, records file metadata, stores non-empty raw JSONL lines in SQLite while preserving malformed lines with parse errors, and rebuilds the raw event FTS index for later diagnostics. Derived parser extraction, git correlation, proof reports, and final exit-code behavior are still planned.

Start here:

- [Documentation index](docs/README.md)
- [CLI behavior](docs/cli.md)
- [Product requirements](docs/prd.md)
- [Architecture notes](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Operating model](docs/operating-model.md)

## Non-Negotiable Design Principle

Every feature must improve:

```bash
prooflog proof --since main
```

If a feature does not make that command more trustworthy, faster, clearer, or easier to adopt, defer it.
