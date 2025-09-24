# Contributing to bop-aof Documentation

## Workflow Overview
1. Create a topic branch with a descriptive name (e.g., `docs/m2-api-guide-draft`).
2. Draft content under `docs/bop-aof/` or `docs/aof2/` using the approved templates in `docs/templates/`.
3. Run style checks locally before opening a pull request.

## Required Checks
- Execute the markdown lint helper:
  ```bash
  ./scripts/docs_markdownlint.sh
  ```
  The script runs `markdownlint-cli` with repo defaults (long-line and raw HTML relaxed). Install Node.js/npm if `npx` is unavailable.
- Alternatively, use the bake integration:
  ```bash
  ./bake docs-lint
  ```
- If you modify diagrams or binary assets, include regenerated sources under `docs/bop-aof/media/` and reference them in the related doc.
- CI enforces the same checks via `.github/workflows/docs-lint.yml`; ensure your branch passes locally before pushing.

## Pull Request Expectations
- Tag the PR with `docs:bop-aof` and request review from the Docs Lead plus the owning stakeholder listed in `m1_foundation_summary.md`.
- Update the changelog tables embedded in each doc with dates and authors.
- Note follow-up items in `docs/bop-aof/progress_tracker.md` when scope moves to later milestones.

## Escalation
Questions or blockers should be raised in the weekly Tuesday documentation sync or via the `#bop-docs` internal channel. For urgent issues (e.g., lint script failures in CI), escalate to the Storage Platform Docs owner.
