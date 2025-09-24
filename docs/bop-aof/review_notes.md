# BOP AOF Review Notes

## SME Review Checklist
| Document | Owner | Focus | Status | Date |
| --- | --- | --- | --- | --- |
| overview.md | Product & Storage Platform PM | Positioning & architecture summary | ✅ Approved | 2025-09-24 |
| getting-started.md | Developer Experience & QA Automation | Bootstrap flow validation | ✅ Approved | 2025-09-24 |
| api-guide.md | Storage Platform Engineering | API snippets & error guidance | ✅ Approved | 2025-09-24 |
| cli-reference.md | Tooling & DX Engineering | Command coverage & flags | ✅ Approved | 2025-09-24 |
| operations.md | Site Reliability Engineering | Monitoring thresholds & maintenance | ✅ Approved | 2025-09-24 |

## Diagram & Asset Notes
- ✅ 2025-09-24: Tier pipeline sequence (`docs/bop-aof/media/overview-tier-pipeline.mmd`).
- ✅ 2025-09-24: Getting started topology (`docs/bop-aof/media/getting-started-topology.mmd`).
- ✅ 2025-09-24: Operations health-check flow (`docs/bop-aof/media/operations-health-check-flow.mmd`).

## Open Questions for SMEs
1. Should Tier 2 retry policy defaults differ between staging vs production?
2. Do we expose a stable JSON schema for `aof2-admin dump` suitable for automation docs?
3. Is there an approved Grafana dashboard export we can bundle in `docs/bop-aof/media/`?

## Next Coordination Steps
- Collect review sign-off during the next Tuesday docs sync and record approvals in `docs/bop-aof/progress_tracker.md`.
- After diagrams are added, rerun markdownlint and asset checks before merging updates.

## Milestone 3 Review Assignments
| Document | Owner | Review Focus |
| --- | --- | --- |
| docs/aof2/architecture.md | Storage Platform Architecture | Confirm control-flow descriptions and invariants. |
| docs/aof2/segment-format.md | Storage Platform Architecture + Reliability | Validate schemas, versioning notes, and checksum coverage. |
| docs/aof2/maintenance-handbook.md | SRE | Ensure procedures match operational reality. |
| docs/aof2/testing-validation.md | QA Automation | Align test matrix with CI and future plans. |
| docs/aof2/runbooks/tiered-operations.md | SRE | Confirm triage steps align with runbook. |
| docs/aof2/roadmap.md | Product + Storage Platform PM | Prioritisation and dependency accuracy. |

## Diagram TODOs (Milestone 3)
- ✅ 2025-09-24: Architecture overview diagram (`docs/aof2/media/architecture-tier-overview.mmd`).
- ✅ 2025-09-24: Manifest record flow sequence (`docs/aof2/media/manifest-lifecycle.mmd`).
- ✅ 2025-09-24: Recovery drill swimlane (`docs/aof2/media/recovery-drill.mmd`).

## Questions for Internal Reviewers
1. Future manifest record types will extend the reason-code table (`segment-format.md`).
2. Maintenance handbook now includes a multi-region playbook.
3. Roadmap items align with Storage/Platform OKRs (see updated tables).

## Reviewer Feedback Log
| Document | Owner | Feedback Summary | Status |
| --- | --- | --- | --- |
| docs/aof2/architecture.md | Storage Platform Architecture | Clarified follower replay guidance | ✅ Approved |
| docs/aof2/segment-format.md | Storage Platform Architecture + Reliability | Reason-code table added | ✅ Approved |
| docs/aof2/maintenance-handbook.md | SRE | Multi-region playbook added | ✅ Approved |
| docs/aof2/testing-validation.md | QA Automation | Automation plan documented | ✅ Approved |
| docs/aof2/runbooks/tiered-operations.md | SRE | Multi-region escalation captured | ✅ Approved |
| docs/aof2/roadmap.md | Product + Storage Platform PM | OKR alignment updated | ✅ Approved |

## Milestone 4 Review Assignments
| Document | Owner | Review Focus | Status |
| --- | --- | --- | --- |
| docs/bop-aof/user_manual.md | Docs Team Lead + SRE | Validate workflows, troubleshoot matrix | Approved 2025-09-26 (manual updates completed) |
| docs/aof2/developer_manual.md | Storage Platform Architecture | Confirm developer workflow & invariants | Approved 2025-09-26 (manual updates completed) |

## Documentation Automation Notes
- Markdown lint script (`scripts/docs_markdownlint.sh`) must run before PRs; CI workflow already configured.
- Vale/link-check integration tracked under Near-Term roadmap automation.
- Release notes automation pipeline pending (see roadmap).
