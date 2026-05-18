# Product Requirements

## Problem

Coding agents can now make large, real changes to production repositories, but the evidence of what changed, what failed, what passed, and what remains risky is scattered across local Codex transcripts, terminal output, diffs, approvals, and memory.

Senior engineers need a fast way to decide whether agent-assisted work is proven enough to review, merge, or hand off.

## Target User

A senior engineer, staff engineer, maintainer, or platform operator who uses Codex heavily on real repos and remains accountable for correctness, security, release readiness, and reviewer trust.

## Core Command

```bash
prooflog proof --since main
```

## User Story

As a senior engineer, before opening or reviewing an agent-assisted PR, I want a local proof report that shows changed files, relevant Codex sessions, passed verification commands, unresolved failures, risky areas, and a clear readiness decision.

## MVP Scope

- Rust CLI
- SQLite database
- Codex-only JSONL ingestion
- Raw-first event storage
- Parser fixtures from local Codex traces
- Git diff correlation
- Verification, failure, and risk detectors
- Text and Markdown reports
- Exit codes:
  - `0` READY
  - `1` NOT READY
  - `2` UNKNOWN
  - `3` runtime ProofLog error

## Non-Goals

- Dashboard
- Multi-agent support
- Cloud upload
- Launching Codex
- Approving commands
- Semantic summaries
- Embeddings
- AGENTS.md generation
- Enterprise governance

## Success Metric

ProofLog is worth continuing only if senior engineers use it before trusting Codex-assisted work.
