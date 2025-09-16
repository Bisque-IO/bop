# TACK — Tiny Agentic Context Kit (Moontrade)

**Purpose.** Keep agentic development *lightweight and local*: only files and folders in-repo. No external apps. Context is close to code. Every task ships with a small, copy-pasteable “context pack”.

**Principles**
- **Small, sharp docs.** Anything longer than a screen goes into links or an ADR.
- **Single source for a task.** `ctx.md` holds the spec the implementer and an LLM will use.
- **Acceptance is explicit.** `done.md` is the merge gate.
- **Context on demand.** `merge-context.sh` stitches a one-file pack in `/contexts/`.
- **Everything linkable.** Prefer relative links to code and ADRs.

---

## Roles (human or AI—same contract)

- **Owner**  
  Sets direction, accepts or rejects scope tradeoffs for an epic. Guards nonnegotiables.

- **Maintainer**  
  Custodian of architecture and preferences; writes/approves ADRs; ensures interfaces stay clean.

- **Implementer**  
  Builds the story. Must keep `ctx.md` and code aligned. Writes tests first or together with code.

- **Reviewer**  
  Uses `pr-template.md` to check acceptance, interfaces, and tests. Flags architectural drift.

- **QA**  
  Runs smoke tests from `qa-checklist.md` and verifies `done.md` items. Can be same person as Reviewer on small teams.

- **Scribe (optional)**  
  Keeps `links.md` updated (PRs, issues), curates notes that future readers need.

> Tip: for solo work, you still “wear the hats”. Mark who played which role in PR description.

---

## Layout (what lives where)

- `00_project/` — mission, hard constraints, terminology.
- `10_arch/` — one-screen overview, public interfaces, ADRs.
- `20_prefs/` — code style deltas, stack & versions, testing conventions.
- `30_backlog/` — epics with stories. Each story = a folder with:
  - `ctx.md` — *context pack* (the spec you hand to a human/LLM)
  - `done.md` — acceptance checklist (merge gate)
  - `notes.md` — implementation hints, gotchas (brief)
  - `links.md` — PRs, issues, code paths
- `40_reviews/` — PR and QA checklists.
- `50_ops/` — runbooks and observability cheats.

- `/contexts/` — generated one-file packs for copy-paste (ignored by git if you want).
- `/.tack/` — tiny local scripts (optional).

---

## Workflow (micro-ritual)

1. **Open a story**: create `30_backlog/epic-*/story-*/` and fill `ctx.md` (≤1 screen) + `done.md` (3–7 checkboxes).
2. **Generate pack**: run `/.tack/merge-context.sh <story-dir>` → `contexts/story-*.md`.
3. **Implement**: keep `ctx.md` and code in sync; link code paths in `links.md`.
4. **Review**: PR body includes the generated pack; reviewer uses `40_reviews/pr-template.md`.
5. **Merge**: only when `done.md` is fully checked and CI/benchmarks meet constraints.

---

## Story Context Pack (template)

Copy this into `30_backlog/epic-*/story-*/ctx.md`.

```md
# Story <num> — <title> (Owner: <name>)

## Why (1–2 sentences)
<What user/system outcome this enables.>

## Definition (concrete bullets)
- Input: <topic/api> (<schema/version>)
- Output: <api/event/artifact>
- Perf/SLA: <throughput/latency/memory/availability>
- Compatibility: <targets> (x86_64, aarch64; Linux/macOS/Windows)
- Failure semantics: <idempotency/replay/exactly-once/etc.>

## Constraints
- <nonnegotiable 1>
- <nonnegotiable 2>
- Safe Rust; no `unsafe` (unless ADR says otherwise)

## Interfaces (copy-paste)
- CLI: `<cmd ...>`
- API: `POST /...`
- Library:
```rust
pub fn <name>(...) -> Result<...> { /* contract only */ }
```

## Data shapes (tiny)
```rust
pub struct <Type> { /* fields that matter */ }
```

## Tests to pass
- <test 1>
- <test 2>
- Bench: <target> on <machine>, p95 < X ms

## Where to put code
- `<path/to/crate_or_module>`
- Wire at `<path/to/integration/point>`

## Links
- Arch overview: ../../10_arch/overview.md#<section>
- ADR: ../../10_arch/decisions/<id>.md
```

**Acceptance (`done.md`)**
```md
- [ ] Tests cover <key behaviors> and guard the SLA
- [ ] Interfaces match `ctx.md` exactly (names, shapes, errors)
- [ ] Metrics/logs added (list them)
- [ ] PRs/issues linked in `links.md`
```

---

## Review & QA

**PR template (`40_reviews/pr-template.md`)**
```md
### Context
Paste from `/contexts/story-xxx-*.md`

### Checks
- [ ] All `done.md` items checked
- [ ] Interfaces unchanged vs `ctx.md` (or ADR updated)
- [ ] Tests prove constraints; CI green
```

**QA Checklist (`40_reviews/qa-checklist.md`)**
```md
- [ ] Happy path works with sample data
- [ ] Failure path produces expected errors/logs
- [ ] Perf smoke meets stated p95 locally (note hardware)
```

---

## Rust Defaults (Moontrade)

- **Stack (`20_prefs/stack.md`)**: tokio, tracing, thiserror, anyhow, serde, rdkafka, reqwest; pin versions that matter.
- **Style (`20_prefs/code-style.md`)**: `clippy --deny` for `unwrap_used`, `expect_used`, `todo`, `panic`; Rustfmt defaults unless noted.
- **Testing (`20_prefs/testing.md`)**: unit + 1 criterion bench + 1 integration test w/ golden file.

---

## Tiny Scripts (optional)

`/.tack/merge-context.sh` stitches pack → `/contexts/story-*.md`.  
`/.tack/new-story.sh` scaffolds a new story folder.

> You can `.gitignore` `/contexts/*` if you treat packs as throwaways.

---

## Guardrails

- If a change deviates from `ctx.md`, either update `ctx.md` *or* write/append an ADR—don’t drift silently.
- Keep `ctx.md` under one screen; if it grows, link out.
- Prefer relative links into the repo over prose repetition.
