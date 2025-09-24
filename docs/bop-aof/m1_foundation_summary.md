# Milestone 1 – Foundation & Discovery
_Status: Completed — 2025-09-23_

## Stakeholder Matrix
| Doc Set | Key Artifacts | Primary Owner | Supporting Stakeholders | Notes |
| --- | --- | --- | --- | --- |
| Public Overview & Positioning | docs/bop-aof/overview.md, launch notes | Docs Team Lead | Product Management, Storage Platform PM | Docs lead curates messaging; product validates positioning and release messaging. |
| Getting Started & Quickstart | docs/bop-aof/getting-started.md, onboarding tutorials | Developer Experience | Docs Team, QA Automation | DevEx owns tooling accuracy; QA confirms install flows across targets. |
| API & Integration Guide | docs/bop-aof/api-guide.md, SDK snippets | Storage Platform Engineering | Docs Team, Runtime/SDK Maintainers | Storage engineering ensures API parity; runtime team reviews async/backpressure semantics. |
| CLI & Tooling Reference | docs/bop-aof/cli-reference.md, admin examples | Tooling & DX Engineering | SRE, Docs Team | Tooling keeps examples in sync with binaries; SRE contributes operational flags and guardrails. |
| Operations & Runbooks | docs/bop-aof/operations.md, docs/aof2/runbooks/* | Site Reliability Engineering | Storage Platform, Support Enablement | SRE maps alerts to actions; support captures field escalations. |
| Internal Architecture & Roadmap | docs/aof2/architecture.md, docs/aof2/segment-format.md, docs/aof2/roadmap.md | Storage Platform Architecture | Docs Team, Reliability Architecture | Architecture preserves design intent; docs team normalizes format and terminology. |
| User & Developer Manuals | docs/bop-aof/user_manual.md, docs/aof2/developer_manual.md | Docs Team Lead | Storage Platform, SRE, Support Enablement | Manuals bridge public/internal sets; reviews aligned with Milestone 4 gate. |

## Review Cadence & Tooling Workflow
- Weekly 30-minute documentation sync each Tuesday over the first three milestones; attendance: Docs Lead, PM, area owners.
- Async reviews via GitHub PRs requiring one approval from Docs and one from the owning team; PRs tagged with `docs:bop-aof` label.
- Drafts live in feature branches; merge requires checklist covering template use, link validation, and terminology alignment.
- CI gate: reuse existing repo checks plus new `npx markdownlint "docs/**/*.md"` pre-merge step (local today, tracked for CI automation in follow-ups).
- Templates and shared assets live under `docs/templates/`; diagrams versioned alongside source in `docs/bop-aof/media/` (to be created with first diagram).
- Meeting notes and action items recorded in `docs/bop-aof/progress_tracker.md` change history for transparency.

## Source Inventory & Reuse Notes
### docs/aof2/
| Source | Coverage Snapshot | Reuse Plan | Identified Gaps |
| --- | --- | --- | --- |
| docs/aof2/aof2_store.md | End-to-end tiered store architecture (tiers 0–2) | Forms backbone of internal architecture chapter; diagrams will be redrawn to match template. | Needs updated Tier 0 queue metrics and clarified failover story. |
| docs/aof2/aof2_manifest_log.md | Binary manifest format, lifecycle, replay steps | Reused in API appendix for manifest tooling and internal design deep dive. | Add examples that map manifest entries to CLI commands. |
| docs/aof2/aof2_runbook.md | End-to-end operational checklist with failure modes | Migrated into runbook template for ops manual; keep troubleshooting scripts. | Current flow is prose-heavy; restructure into numbered procedures. |
| docs/aof2/aof2_progress.md & aof2_implementation_plan.md | Historical rollout planning, task statuses | Summarizes lessons learned for internal roadmap context; informs risk appendix. | Contains stale timelines; mark historic and link to new tracker. |
| docs/aof2/aof2_tracing.md & aof2_read_path.md | Telemetry spans and read path behaviour | Provide detailed references in API guide and operations doc. | Expand to cover async back-pressure metrics and new tracing spans. |

### Related Platform Docs
| Source | Relevance | Reuse Plan | Gap / Action |
| --- | --- | --- | --- |
| docs/platform_overview.md | High-level positioning for BOP platform | Anchor for public overview introduction and terminology glossary. | Update terminology to reference AOF2 naming once glossary finalizes. |
| docs/architecture.md | Legacy architecture diagrams for bopd/libbop | Supplies contextual visuals for manuals and public overview. | Needs refresh to highlight AOF2 components and I/O paths. |
| docs/usockets_design.md & docs/RAFT.md | Networking and consensus background | Cross-linked in internal design and API guide for dependency context. | Consolidate formatting to new internal design template. |
| docs/bmad_method.md & docs/tack-method.md | Operational playbooks, methodology | Provides structure ideas for future ops runbooks. | Extract reusable sections into runbook template during M2. |

## Template Approvals
| Template | File | Owning Team | Notes |
| --- | --- | --- | --- |
| Public documentation | docs/templates/public_doc_template.md | Docs Team | Approved for all public-facing pages starting M2; includes changelog block for release history. |
| Internal design | docs/templates/internal_design_template.md | Storage Platform Architecture | Approved for architecture updates and ADR-style docs; aligns with architecture review expectations. |
| Runbook / operations | docs/templates/runbook_template.md | Site Reliability Engineering | Approved for operational procedures and on-call guides; SRE to seed first conversion in M2. |

## Follow-Ups
- Automate `markdownlint` (and future Vale checks) in CI once tooling container is defined. Local entrypoints (`scripts/docs_markdownlint.sh`, `./bake docs-lint`) are ready; `.github/workflows/docs-lint.yml` now runs on PRs.
- Docs Lead to circulate recurring Tuesday sync invite and confirm alternates for holidays.
- PM to stand up lightweight GitHub Project board (`bop-aof-docs`) to track milestone tasks.
- Storage Platform to refresh Tier 0 metrics notes before internal architecture drafting begins.
- SRE to pilot runbook template by reformatting `docs/aof2/aof2_runbook.md` ahead of Milestone 2 drafting. ✔ Initial conversion completed 2025-09-23; gather feedback after first incident review.
