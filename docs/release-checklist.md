# Release Checklist

Use this checklist before tagging or publishing a ProofLog release.

The release is not ready until every required check is complete and any skipped check has a written reason.

## 1. Confirm Scope

- [ ] Confirm every release change improves `prooflog proof --since main`.
- [ ] Confirm README, docs, and CLI help describe implemented behavior only.
- [ ] Confirm non-goals remain intact: no dashboard, cloud sync, multi-agent orchestration, semantic search, embeddings, or Codex launcher scope.
- [ ] Confirm known limitations are documented.

## 2. Local Build And Test

Run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Parser fixture snapshots must pass:

```bash
cargo test --test parser_fixtures
```

If snapshots intentionally changed, update and review them explicitly:

```bash
INSTA_UPDATE=always cargo test --test parser_fixtures
```

## 3. Demo And README Verification

- [ ] Run the README quickstart from a normal shell.
- [ ] Verify `prooflog --help` lists the implemented command surface.
- [ ] Verify `prooflog doctor` gives actionable local readiness output.
- [ ] Verify `prooflog doctor --parser` prints count-only diagnostics.
- [ ] Verify `prooflog proof --since main` returns the expected decision for a known local test repository.
- [ ] Verify Markdown output is PR-pasteable:

```bash
prooflog proof --since main --format md > prooflog.md
```

- [ ] Verify JSON output parses:

```bash
prooflog proof --since main --format json > prooflog.json
```

## 4. Install Smoke Check

Install from the current checkout into a temporary root:

```bash
cargo install --path . --root /tmp/prooflog-install-smoke
/tmp/prooflog-install-smoke/bin/prooflog --help
```

For a stronger check, repeat from a clean clone before tagging.

## 5. Privacy And Fixture Review

- [ ] Review `tests/fixtures/codex/` for secrets, tokens, account IDs, private hostnames, customer names, local usernames, and private paths.
- [ ] Review `tests/snapshots/` for raw transcript content or command output that should not be public.
- [ ] Run a public metadata/secret scan over README, docs, source, tests, and fixtures.
- [ ] Confirm reports redact obvious secret-like values.
- [ ] Do not paste private local Codex evidence into release notes.

## 6. Maintainer Local History Check

Run against maintainer-owned local Codex history before release:

```bash
prooflog init
prooflog ingest --codex --codex-root ~/.codex
prooflog proof --since main
```

This check is local-only. Release notes may summarize the result, but must not include private transcript text, raw JSONL, command output, local paths, customer names, or secrets.

## 7. Version And Tag Gate

Before tagging:

- [ ] Confirm `Cargo.toml` version is correct.
- [ ] Confirm `Cargo.lock` is current.
- [ ] Confirm docs mention only available install paths and clearly label planned package channels.
- [ ] Confirm the working tree is clean.
- [ ] Confirm the release commit is the intended commit.

Tag only after the checks above pass:

```bash
git tag vX.Y.Z
```

Do not push the tag until release notes and publish decisions are final.

## 8. Publish Gate

Publishing is optional and must be explicit.

Before publishing to a package registry:

- [ ] Confirm package metadata is complete.
- [ ] Confirm package contents do not include private files.
- [ ] Confirm install docs match the published artifact.
- [ ] Confirm rollback steps are known.

If package publishing is not ready, ship the source-based release only and keep `cargo install prooflog` documented as planned.

## 9. Post-Release

- [ ] Verify the public tag or release points at the intended commit.
- [ ] Verify install instructions still work for the chosen release path.
- [ ] Update the Homebrew tap formula with the release archive URL and sha256.
- [ ] Run the tap checks before opening or merging tap changes.
- [ ] Record any known limitations in public release notes.
- [ ] Create follow-up issues for release problems instead of hiding them in private notes.
