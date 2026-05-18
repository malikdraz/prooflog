# Repository Instructions

## Product Boundary

ProofLog is a local-first Rust + SQLite CLI for proof reports around agent-assisted code changes.

The core command is:

```bash
prooflog proof --since main
```

Current commands are `init`, `doctor`, `ingest`, and `proof`. Default local state lives under `~/.prooflog`:

- `~/.prooflog/config.toml`
- `~/.prooflog/prooflog.db`

Keep the product focused on local session JSONL ingestion, raw-first storage, git correlation, verification/failure/risk classification, deterministic reports, useful exit codes, privacy, and redaction.

Out of scope before the adoption test: dashboard, Tauri UI, cloud sync, multi-agent support, semantic search, embeddings, launching external tools, and generating this file.

## Implementation Rules

- Use Rust for project code and `clap` for CLI behavior.
- Preserve raw events; derived tables must be rebuildable.
- Prefer `UNKNOWN` over false `READY`.
- Add or update parser fixtures before parser behavior changes.
- Keep storage local by default and owner-only on Unix-like systems.
- Do not print secrets, raw transcript content, raw JSONL, raw command output, or raw local paths by default.
- Every feature must improve `prooflog proof --since main` by making it more trustworthy, clearer, faster, or easier to adopt.

## Tests And Checks

Use focused tests first, then broaden based on risk.

Required before completing implementation work:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Parser changes need fixture and snapshot coverage. Report changes need deterministic snapshot or CLI integration coverage. Release automation changes need script-level tests and a release-check dry run where practical.

## Release Workflow

`Cargo.toml` is the canonical version source. Release tags must be stable semver with a `v` prefix, such as `v0.1.1`.

Use `scripts/release.sh` for release operations:

- `next patch|minor|major`
- `prepare patch|minor|major|X.Y.Z`
- `verify-tag`
- `extract-notes`
- `publish-tap`

The GitHub release workflow validates the tag, extracts release notes from `CHANGELOG.md`, runs the release gate, creates the GitHub release, and publishes the Homebrew tap formula using `HOMEBREW_TAP_TOKEN`.

Do not rewrite published release tags. If a public tag is wrong, ship the next patch version.

## Homebrew Tap

The tap repository is `malikdraz/homebrew-tap`, installed as `malikdraz/tap`.

ProofLog owns the release; the tap owns the install recipe. Source-built tap formulas must not keep stale `bottle do` blocks unless bottles are intentionally produced and published.

After tap publication, verify:

```bash
brew update
brew reinstall malikdraz/tap/prooflog
prooflog --help
```

Also run a temp-HOME `prooflog init` smoke test and confirm it creates `~/.prooflog/config.toml` and `~/.prooflog/prooflog.db`.

## Documentation

Keep public docs accurate and public-safe:

- User-facing CLI behavior: `docs/cli.md`
- Installation behavior: `docs/installation.md`
- Release process: `docs/release-checklist.md`
- Product and architecture context: `docs/prd.md`, `docs/architecture.md`, `docs/roadmap.md`, `docs/risks.md`

Do not include private planning metadata, private issue IDs, raw local evidence, secrets, or unpublished implementation claims in public docs.

## Done Criteria

A change is done only when:

- Code compiles and required checks pass.
- Changed behavior is tested.
- CLI output remains deterministic.
- Public docs and changelog are updated when user-facing behavior changes.
- Privacy impact has been considered.
- Release or tap changes are verified against the actual GitHub/Homebrew surfaces when applicable.
