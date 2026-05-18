# Demo Script

This is the target first-demo flow. `init`, `doctor`, JSONL file discovery, raw JSONL line storage, raw/message/command-output FTS indexing, session/message/command/approval/file-change derivation, proof-command git context plus changed-file detection, and session-to-repo correlation are implemented; richer derived extraction and proof report generation are still planned.

```bash
cargo install --path .

prooflog init

prooflog doctor

prooflog ingest --codex --codex-root ~/.codex

prooflog proof --since main

prooflog proof --since main --format md > prooflog.md
```

## Expected Doctor Output

```text
prooflog doctor

Config:
  path: /home/user/.config/prooflog/config.toml
  db: /home/user/.local/share/prooflog/prooflog.db
  codex root: /home/user/.codex
  redaction: secrets=true, local_paths=true

Storage:
  db: /home/user/.local/share/prooflog/prooflog.db
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
  repo: example-project
  branch: feature/example-change
  range: main..HEAD

Changed:
  files: 18
  risky areas:
    auth
    config

Codex evidence:
  sessions: 3
  commands: 47
  approvals: 4

Verification:
  PASS go test ./...
  PASS make lint
  FAIL npm run lint
       unresolved

Decision:
  NOT READY

Why:
  observed unresolved lint failure after agent edits

Next:
  rerun npm run lint or fix the failure
```
