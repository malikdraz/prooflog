# Operating Model

## Weekly Rhythm

- Monday: review open issues, close scope creep, pick milestone work.
- Wednesday: parser fixture review and detector tuning.
- Friday: demo `prooflog proof --since main` on a real branch.

## Definition Of Done

A ticket is done only when:

- code compiles
- tests pass
- fixture coverage exists for parser changes
- output is deterministic
- docs or README are updated if CLI behavior changes
- privacy implications are considered
- no new dashboard-only behavior is introduced

## Review Rule

Every PR must answer:

- Does this improve `prooflog proof --since main`?
- Does this preserve raw events?
- Does this avoid false READY decisions?
- Does this keep install-to-first-report under 5 minutes?

## Release Rule

No release unless:

- `cargo test` passes
- snapshot tests pass
- `prooflog doctor` works
- `prooflog ingest --codex` works on representative fixtures
- `prooflog proof --since main` works in a real repo
- README demo is accurate

## Project Health Metrics

Track only:

- Core command usable? yes/no
- Install-to-first-report time
- Number of fixtures passing
- Number of parser unknowns
- Number of unresolved P0 tickets
- Number of real proof reports generated before review
- Number of external users who reached first report

Avoid vanity metrics:

- number of sessions indexed
- number of dashboard views
- number of possible integrations
- number of planned adapters
