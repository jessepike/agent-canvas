# Vellum — Project Context

## What this is

A personal desktop Markdown editor where blocks can run, the file stays plain, and history is honest. Tauri + Rust core + ProseMirror UI. Built for one person (the owner) to use daily. Open source from commit 1. Apache 2.0.

**Single-user-by-design.** No buyer, no GTM, no design partners. Out-of-scope for any session: positioning, PMF, marketing, buyer adjacency.

## Read first (in this order)

1. `intent.md` — destination, why it matters, shape constraints, stance.
2. `vellum-spec-v0.3.md` — locked design spec (618 lines). The source of truth for everything architectural.
3. `decisions.md` — 20 commitments with confidence tags.
4. `BACKLOG.md` — current work queue (P0 + 30A + 30B + 60 + 90 + v1.5).
5. `status.md` — where things stand.
6. `lessons.md` — recent learnings.
7. `review-findings.md` — durable artifact from the 4-cycle / 2-external-round review. Useful for context on WHY the spec landed where it did.

## Current stage

**Design complete. Develop starting.** The spec is locked through 4 cycles + 2 external multi-model rounds. Critical=0, High=0 at exit. Next move is Phase 0 scaffold then Gate 30A (parser + format-preservation corpus). See BACKLOG.

## The load-bearing wall

If you forget everything else, remember this: **the format-preservation corpus is the v1 build gate**. Editor work does not begin until the corpus passes byte-identical at both the steady-state AND cold-state gates. The parser + partition contract ship FIRST. Spec §Block boundary reconstruction is canonical.

## Architectural anchors (do not violate without re-opening the spec)

- ProseMirror is authoritative for live editing. Rust does NOT maintain a parallel editing tree.
- Block identity is dual: primitives carry YAML `id` on disk; non-primitives carry PM-decorated UUIDs in-memory + sidecar `identity.json`.
- Source preservation is byte-level. No pretty-print. No normalize-on-save. Edit one paragraph, every other block emits its preserved raw bytes.
- Trust is one file: `~/.vellum/trust.toml`. `bind` field anti-spoofs MCP server identity. Evaluation: server gate → tool gate → defaults.
- Save guard: stat+hash before `rename(2)`. If on-disk hash differs from base, abort to three-way conflict.
- IPC: typed block-grain. `ts-rs` for TS types + handwritten Zod for runtime validation.

## What is NOT in scope for v1

- Tantivy search UI, evidence bundle export, plugin marketplace, multiplayer, encryption at rest, git integration, charts, doctor command.
- `vellum:agent`, `vellum:transform`, `vellum:include` — v1.5.
- iOS reader — v1.5.
- Notarization and code-signing — v1.1 (v1.0 ships ad-hoc/dev-signed; documented in README).
- OAuth flows / refresh-token rotation in MCP auth — v1.5.

## Working agreements

- **Spec is locked.** If a change to the architecture seems necessary, open a decision ledger entry in `decisions.md` first; do not silently amend the spec.
- **The corpus is non-negotiable.** Any feature that risks corpus regression must include a corresponding corpus addition.
- **Atomic commits at completed units.** Conventional commits (`type(scope): description`). Never end a session with a dirty tree.
- **Layer A timeline.** Solo agentic build. 0.1–0.3× of 2022 baselines for build velocity. Owner manages calendar cadence; agents sequence dependencies and exit conditions.

## Delegation patterns

- Heavy implementation work → delegate to Codex via the `codex-delegate` skill. Pass a self-contained prompt referencing spec sections.
- Architecture / design questions → CTO agent.
- PMF or positioning concerns → NOT APPLICABLE (single-user-by-design).
- Dev system / harness / agent changes → Forge.

## Files NOT to touch

- `vellum-spec-v0.3.md` — sign-off applied. Open a decision in `decisions.md` first.
- `intent.md` — destination-only; never auto-edit. Surface tensions for owner decision.

## Files safe to edit

- `BACKLOG.md`, `status.md`, `lessons.md`, `decisions.md` (append-only on existing entries).
- Source code under `crates/` and `ui/` once they exist.
- `README.md`, `CONTRIBUTING.md`, `LICENSE`, etc.
