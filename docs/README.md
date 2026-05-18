# ProofLog Documentation

This directory is the repo-local starting point for ProofLog context.

## Read Order

1. [Product requirements](prd.md)
2. [Architecture](architecture.md)
3. [Roadmap](roadmap.md)
4. [CLI behavior](cli.md)
5. [Parser fixtures](parser-fixtures.md)
6. [Risk register](risks.md)
7. [Operating model](operating-model.md)
8. [Demo script](demo.md)

## Current Repository State

The repository currently contains the initial Rust CLI skeleton. It builds and exposes the planned MVP commands, but the command handlers are placeholders until the roadmap items add config, storage, ingestion, parser extraction, git correlation, reports, and exit codes.

## Decision Rule

If a proposed change does not make `prooflog proof --since main` more trustworthy, faster, clearer, or easier to adopt, defer it.
