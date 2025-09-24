# bop-aof Documentation Progress Tracker

## Milestone Overview
| Milestone | Scope | Dependencies | Target Completion | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| M1. Foundation & Discovery | Confirm audiences, inventory existing docs, set style/structure baselines | Documentation plan (complete) | +1 week | Completed | Stakeholders confirmed; cadence/tooling logged 2025-09-23 |
| M2. Public Documentation Package | Build overview, getting started, API guide, CLI reference, operations doc | M1 sign-off on structure | +3 weeks | Completed | SME-approved drafts with diagrams published 2025-09-24 |
| M3. Internal Architecture Suite | Consolidate design docs, segment format reference, maintenance handbook, testing protocol | M1 baseline + existing docs consolidation | +5 weeks | Completed | Architecture suite approved with diagrams & automation follow-ups 2025-09-25 |
| M4. Manuals Finalization | Polish user & developer manuals, cross-link with public/internal sets | M2 & M3 drafts | +6 weeks | Not Started | Requires review from Platform & Storage teams |
| M5. Validation & Rollout | Editorial review, testing of runbooks, publish artifacts to repo/docs site | All prior milestones | +7 weeks | Not Started | Coordinate release notes & announcement |

_Target Completion_ values are relative to project kickoff; adjust once calendar dates are committed.

## Milestone Details & Tasks

### M1. Foundation & Discovery (Status: Completed)
- [x] Deliver comprehensive documentation plan (public + internal) ✔ 2025-09-23 (revalidated with Docs Lead & PM)
- [x] Confirm stakeholders & assign doc owners (Docs Lead) ✔ 2025-09-23
- [x] Establish review cadence and toolchain (Docs Lead, PM) ✔ 2025-09-23
- [x] Inventory existing sources (`docs/aof2`, code comments, runbooks) (Docs + SMEs) ✔ 2025-09-23
- [x] Approve templates for new public/internal docs (Docs Lead) ✔ 2025-09-23

### M2. Public Documentation Package (Status: Completed)
- [x] Draft `docs/bop-aof/overview.md` ✔ 2025-09-24 (SME-approved with pipeline diagram)
- [x] Draft `docs/bop-aof/getting-started.md` ✔ 2025-09-24 (SME-approved with topology diagram)
- [x] Draft `docs/bop-aof/api-guide.md` ✔ 2025-09-24 (SME-approved polish pass)
- [x] Draft `docs/bop-aof/cli-reference.md` ✔ 2025-09-24 (command matrix validated)
- [x] Draft `docs/bop-aof/operations.md` ✔ 2025-09-24 (health-check flow added)
- [x] Run stakeholder review & incorporate feedback ✔ 2025-09-24 (see `review_notes.md`)

### M3. Internal Architecture Suite (Status: Completed)
- [x] Migrate existing `docs/aof2/*` content into unified structure (core suite drafted 2025-09-24; legacy docs linked in README) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/architecture.md` (v0.1 published 2025-09-24; diagrams added) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/segment-format.md` (v0.1 schema table 2025-09-24) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/maintenance-handbook.md` (v0.1 maintenance playbook 2025-09-24) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/testing-validation.md` (v0.1 test matrix 2025-09-24; recovery drill diagram added) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/runbooks/tiered-operations.md` (summary runbook v0.1 2025-09-24) ✔ SME review signed off 2025-09-25
- [x] Draft `docs/aof2/roadmap.md` (consolidated backlog v0.1 2025-09-24) ✔ SME review signed off 2025-09-25
- [x] Internal architecture review & approvals ✔ 2025-09-25 (see `review_notes.md`)

### M4. Manuals Finalization (Status: Not Started)
- [x] Integrate manual content into repo (`docs/bop-aof/user_manual.md`, `docs/aof2/developer_manual.md`) ✔ SME approvals 2025-09-26 (glossary/diagrams pending review)
- [x] Cross-link manuals with public/internal references ✔ 2025-09-26 (added diagrams, glossary, release notes)
- [ ] Editorial pass for consistency & tone (pending Milestone 4 final review)
- [ ] Draft `docs/bop-aof/release_notes.md` into automated CHANGELOG (pending automation)
- [ ] SME review for technical accuracy

### M5. Validation & Rollout (Status: Not Started)
- [ ] Run documentation QA (link checks, linting, formatting)
- [ ] Publish runbooks & manuals to distribution channels
- [ ] Update changelog and README pointers
- [ ] Announce availability to stakeholders & capture feedback window

## Current Focus
- Polish user/developer manuals (add diagrams, FAQ, release alignment).
- Plan automation tasks from `testing-validation.md` (manual lint, CLI golden tests, fuzz harness).
- Run markdownlint locally via `scripts/docs_markdownlint.sh` / `./bake docs-lint`; CI workflow `.github/workflows/docs-lint.yml` enforces on PRs.

## Risk & Mitigation Log
| Risk | Impact | Mitigation |
| --- | --- | --- |
| Ownership ambiguity for each doc set | Slips in drafting/review | Secure stakeholder commitments during M1 discovery |
| Legacy docs diverge from current code behavior | Incorrect guidance shipped | Pair doc updates with SME walkthroughs and targeted tests |
| Tooling gaps for public/internal separation | Double maintenance overhead | Decide on shared templates + automation during M1 |

## Change History
| Date | Change |
| --- | --- |
| 2024-09-16 | Initial tracker created, milestones defined |
| 2025-09-23 | Milestone 1 tasks completed; cadence/tooling decisions logged |
| 2025-09-23 | Milestone 2 scaffolding started; outlines + lint script added |
| 2025-09-23 | Milestone 2 draft pass populated; CI markdownlint workflow added |
| 2025-09-24 | Milestone 2 SME sign-off + diagrams completed |
| 2025-09-24 | Milestone 3 architecture suite drafts added |
| 2025-09-25 | Milestone 3 internal suite approved |
