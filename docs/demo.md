# Demo Script

This is the target first-demo flow. It is not implemented yet.

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

DB:
  path: /home/user/.local/share/prooflog/prooflog.db
  sqlite: ok
  fts5: ok
  permissions: owner-only

Codex:
  root: /home/user/.codex
  jsonl files: found

Git:
  repo: /home/user/src/example-project
  branch: feature/example-change
  merge base: main

Status:
  ready
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
