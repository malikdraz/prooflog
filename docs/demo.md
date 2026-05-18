# Demo Script

This is the target first-demo flow. `init`, `doctor`, JSONL file discovery, raw JSONL line storage, raw/message/command-output FTS indexing, session/message/command/approval/file-change derivation, verification/failure/resolution proof facts, proof-command git context plus changed-file detection, risky path and command classification, session-to-repo correlation, conservative READY/NOT READY/UNKNOWN decisions, plain text plus Markdown plus experimental JSON proof reports, and decision-based exit codes are implemented.

```bash
cargo install --path .

prooflog init

prooflog doctor

prooflog ingest --codex --codex-root ~/.codex

prooflog proof --since main

prooflog proof --since main --format md > prooflog.md

prooflog proof --since main --format json > prooflog.json
```

## Expected Doctor Output

```text
prooflog doctor

Config:
  path: /home/user/.prooflog/config.toml
  db: /home/user/.prooflog/prooflog.db
  codex root: /home/user/.codex
  redaction: secrets=true, local_paths=true

Storage:
  db: /home/user/.prooflog/prooflog.db
  sqlite: ok
  migration: 2
  fts5: ok
  journal: wal

Codex:
  root: ok
  path: /home/user/.codex
  jsonl files: 42

Git:
  repo: /home/user/src/example-project
  branch: feature/example-change

Status:
  config ok
```

## Expected Proof Output

```text
PROOFLOG REPORT

Scope:
  repo: /home/user/src/example-project
  branch: feature/example-change
  since: main
  head: abc123
  merge base: def456
  dirty: no

Changed:
  files: 18
  additions: 240
  deletions: 41
  docs only: no
  M src/auth/session.go (+120 -12)

Codex evidence:
Codex:
  relevant sessions: 3
  ambiguous sessions: 0
  session-a Auth fix [workspace, command-cwd]

Verification:
  facts: 3
  passed: 2
  failed: 1
  unknown: 0
  passed session-a go test ./...
  failed session-a npm run lint

Failures:
  failure resolutions: 1
  unresolved: 1
  resolved: 0
  ambiguous: 0
  unresolved session-a npm run lint

Risks:
Risk:
  risk level: elevated
  risky files: 1
  auth: src/auth/session.go (authentication or authorization path)
Risky commands:
  relevant: 0
  ambiguous: 0

Decision:
  status: NOT READY
  reason: unresolved verification failure: session-a npm run lint

Why:
  reason: unresolved verification failure: session-a npm run lint

Next:
  resolve the listed verification failures and rerun proof
```
