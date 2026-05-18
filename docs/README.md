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

The repository currently contains the initial Rust CLI and local config handling. `prooflog init` creates local TOML config, and `prooflog doctor` reads it. Storage, ingestion, parser extraction, git correlation, reports, and final exit-code behavior are still planned roadmap work.

## Decision Rule

If a proposed change does not make `prooflog proof --since main` more trustworthy, faster, clearer, or easier to adopt, defer it.
