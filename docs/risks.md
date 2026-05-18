# Risk Register

## Private Dashboard Drift

Failure mode: ProofLog becomes a place to browse old sessions.

Correction: Delete or defer every feature that does not improve `prooflog proof --since main`.

## Codex JSONL Parser Fragility

Failure mode: Parser assumes one event shape and breaks on real local traces.

Correction: Raw-first storage, unknown event preservation, fixture-driven parser, snapshot tests.

## Privacy Exposure

Failure mode: ProofLog stores, prints, or exports sensitive transcript content unsafely.

Correction: Local-only by default, owner-only DB permissions, redaction checks before export, no cloud.

## Weak Adoption

Failure mode: Users say it is interesting but do not run it before PRs.

Correction: Make the proof command binary, fast, and tied to review readiness.

## Scope Creep

Failure mode: Project expands into multi-agent observability, Tauri UI, search, summaries, or runbooks before the core proof command works.

Correction: Milestone gate: no feature expansion before seven-day adoption test.

## False Confidence

Failure mode: ProofLog says READY when evidence is incomplete.

Correction: Prefer UNKNOWN over READY. READY requires passing verification after relevant changes.

## Command Classification Errors

Failure mode: ProofLog mislabels commands or misses verification evidence.

Correction: Expose reasons, confidence, and detector names. Add fixture before changing detector behavior.
