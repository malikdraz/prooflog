# Installation

This guide installs ProofLog from source and gets to a first local proof report.

## Requirements

- Rust `1.80` or newer
- Git
- A local checkout of this repository
- Local Codex JSONL history, usually under `~/.codex`

ProofLog is currently validated around macOS and Linux-style local paths. Windows support has not been validated yet.

ProofLog stores data locally in SQLite. It does not upload Codex history, git state, command output, or reports.

## Install From Source

```bash
git clone https://github.com/malikdraz/prooflog.git
cd prooflog
cargo install --path .
```

Confirm the binary is on your `PATH`:

```bash
prooflog --help
```

If your shell cannot find `prooflog`, check that Cargo's bin directory is on your `PATH`. It is usually:

```text
$HOME/.cargo/bin
```

## Package Install

This command is planned for a future packaged release, but is not available yet:

```bash
cargo install prooflog
```

Use `cargo install --path .` from this repository until a package is published.

## First Report

From the repository you want to inspect:

```bash
prooflog init
prooflog doctor
prooflog ingest --codex --codex-root ~/.codex
prooflog proof --since main
```

If your Codex history is somewhere else, pass that path explicitly:

```bash
prooflog ingest --codex --codex-root /path/to/codex-history
```

If you want to run the proof command from outside the target repository, pass `--repo`:

```bash
prooflog proof --since main --repo /path/to/repo
```

## Output Formats

Plain text is the default:

```bash
prooflog proof --since main
```

Markdown is useful for PRs:

```bash
prooflog proof --since main --format md > prooflog.md
```

JSON is available for tooling experiments:

```bash
prooflog proof --since main --format json > prooflog.json
```

## Local Files

When `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are set, ProofLog uses:

```text
$XDG_CONFIG_HOME/prooflog/config.toml
$XDG_DATA_HOME/prooflog/prooflog.db
```

Otherwise it falls back to:

```text
$HOME/.config/prooflog/config.toml
$HOME/.local/share/prooflog/prooflog.db
```

You can override the database path when needed:

```bash
prooflog init --db /path/to/prooflog.db
prooflog doctor --db /path/to/prooflog.db
prooflog proof --since main --db /path/to/prooflog.db
```

## Troubleshooting

### `cargo` Is Missing

Install Rust with `rustup`, then reopen your shell:

```bash
rustup --version
cargo --version
```

ProofLog requires Rust `1.80` or newer.

### `prooflog` Is Not Found

Check Cargo's bin directory:

```bash
echo "$PATH"
ls "$HOME/.cargo/bin/prooflog"
```

Add `$HOME/.cargo/bin` to your shell `PATH` if needed.

### `run prooflog init`

`doctor`, `ingest`, and `proof` expect local config and storage. Start with:

```bash
prooflog init
```

### Codex Root Is Missing

Run:

```bash
prooflog doctor
```

If the default Codex root is missing, ingest with an explicit path:

```bash
prooflog ingest --codex --codex-root /path/to/codex-history
```

### Not Inside A Git Repository

Run `prooflog proof` from the repository you want to inspect, or pass `--repo`:

```bash
prooflog proof --since main --repo /path/to/repo
```

### Invalid Base Ref

`--since` must be a valid git ref in the target repository. Common examples:

```bash
prooflog proof --since main
prooflog proof --since origin/main
prooflog proof --since HEAD~1
```

### Permission Warnings

On Unix-like systems, ProofLog expects local config and DB files to be owner-readable/writable only. If `doctor` reports broad permissions, follow the printed `chmod 600` command.

### No Relevant Evidence

If the proof report is `UNKNOWN`, run verification commands from the target repository, ingest Codex history again, then rerun proof:

```bash
prooflog ingest --codex --codex-root ~/.codex
prooflog proof --since main
```

### Parser Diagnostics

For count-only parser health checks:

```bash
prooflog doctor --parser
```

This does not print raw transcript text, raw JSONL content, command output, or parse error text.
