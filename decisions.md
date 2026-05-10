---
project: vellum
updated: 2026-05-10
---

# Vellum — Decisions

Confidence tags: `[H/M/L]` confidence × `[R/D/G/F]` basis (Researched / Deliberated / Gut / Forced).

## Current Commitments Index

- **Stack:** Tauri 2.x + Rust core + ProseMirror-authoritative UI + CodeMirror 6 source view (D-VELLUM-1, 2).
- **License:** Apache 2.0, public from commit 1 (D-VELLUM-3).
- **v1 primitive set:** `vellum:live-query` only (D-VELLUM-4). `vellum:agent`, `vellum:transform`, `vellum:include` deferred to v1.5.
- **Source preservation:** byte-level via partition contract on top of `pulldown-cmark`; format-preservation corpus is the v1 build gate (D-VELLUM-5).
- **Block identity:** dual system — on-disk YAML `id` for primitives; in-memory PM-decorated UUIDs for non-primitives, persisted in sidecar `identity.json` with source-hash auto-migration on rename (D-VELLUM-6, D-VELLUM-12).
- **Trust:** single `~/.vellum/trust.toml` with `bind` field anti-spoofing, 3-step deterministic evaluation order, `unknown_server = "ask"` default (D-VELLUM-7, D-VELLUM-13).
- **IPC contract:** typed block-grain via `ts-rs` for TS types + handwritten Zod schemas; CI type-tests reconcile (D-VELLUM-14).
- **First MCP server:** GitHub MCP read-only subset with token-only auth via OS keychain. OAuth deferred to v1.5 (D-VELLUM-8).
- **v1.0 signing:** ad-hoc / developer-signed only. macOS notarization, Windows code-signing, Linux signature distribution all deferred to v1.1 (D-VELLUM-15).
- **Doctor:** cut from v1; inline validation replaces it (D-VELLUM-9).
- **Cache:** vault-rooted at `<vault-root>/.vellum-cache/<docpath>/`, sidecar-first, `pinned` is default result_policy (D-VELLUM-10).
- **Milestones:** Gate 30A (parser+corpus) → 30B (editor) → 60 (live-query end-to-end) → 90 (honest history + ship) (D-VELLUM-11).

## Decision Log

### D-VELLUM-1 — Tauri 2.x + Rust core [H-R]
**Date:** 2026-05-09 (locked in spec v0.3)
**Decision:** Native desktop shell via Tauri 2.x with Rust core owning filesystem, MCP client, primitive runtime, hashing, cache. WebView UI is a thin shell.
**Why:** Local-by-default rules out Electron's web-app patterns. Rust core gives the durability surface (filesystem atomics, blake3, MCP `rmcp` crate) where it belongs.
**Supersedes:** none.

### D-VELLUM-2 — ProseMirror authoritative for live editing [H-D]
**Date:** 2026-05-09
**Decision:** ProseMirror direct (NOT Tiptap), with a custom schema including primitive node types. Rust does not maintain a parallel editing tree.
**Why:** Two competing live models is the path to corruption bugs. PM owns rendered-view editing; Rust owns source-of-truth bytes. Bidirectional sync is toggle-time, not per-keystroke.

### D-VELLUM-3 — Apache 2.0 open source from commit 1 [H-F]
**Date:** 2026-05-09
**Decision:** Public GitHub repo from the first commit. No private prelude.
**Why:** "Built for me but open" is the operating constraint. Apache 2.0 provides patent grant + permissive use; commit-1 public removes any "should we open source this" inflection point.

### D-VELLUM-4 — v1 ships one primitive only [H-D]
**Date:** 2026-05-09
**Decision:** `vellum:live-query` is the only primitive type in v1. `vellum:agent`, `vellum:transform`, `vellum:include` deferred to v1.5.
**Why:** Single primitive forces correctness on the foundation (schema, runtime, trust, Evidence State, cache, hashing) before scaling primitive variety. Each additional primitive multiplies the surface to test.

### D-VELLUM-5 — Source preservation is byte-level [H-D]
**Date:** 2026-05-09
**Decision:** Format-preservation corpus is built FIRST. Editor work does not start until the corpus passes byte-identical at both steady-state and cold-state gates.
**Why:** If Vellum corrupts files, nothing else matters. Byte-level beats best-effort serializer-equivalent. The corpus is the load-bearing wall and the gate.

### D-VELLUM-6 — Dual block identity system [H-D]
**Date:** 2026-05-10 (review cycle 1; refined cycle 4)
**Decision:** Primitives carry stable YAML `id` (on-disk). Non-primitives get in-memory PM-decorated UUIDs persisted in sidecar `identity.json`. Branch resolution rule: known unchanged → raw bytes; known changed → re-serialize; new → fresh serialize.
**Why:** Source preservation requires answering "is this block the same one I parsed last time?" reliably. Two identity systems handle the two cases (durable primitives vs. ephemeral prose blocks).
**Replaces:** Earlier draft that left non-primitive identity unspecified (cycle 1 Critical).

### D-VELLUM-7 — Trust = single ~/.vellum/trust.toml [H-D]
**Date:** 2026-05-09 (locked); refined 2026-05-10 with `bind` field
**Decision:** Personal config file with three states (`trusted`, `ask`, `block`) per tool/server. No per-doc grant matrix. No dry-run plan UI. Internal capability tracking retained for engine correctness.
**Why:** Single-user constraint cuts the policy-engine UX. The toml + a settings panel that edits it is the entire user-facing trust surface.

### D-VELLUM-8 — First MCP server: GitHub read-only [H-D]
**Date:** 2026-05-10 (cycle 1)
**Decision:** Days 31–60 ships ONE MCP server integration: GitHub MCP server, read-only tool subset. API-token auth via OS keychain. OAuth/device-codes/refresh-token rotation deferred to v1.5.
**Why:** Pinning the first server bounds the auth surface in the milestone. GitHub's token model is the cheapest possible integration.

### D-VELLUM-9 — Vault doctor cut from v1 [H-D]
**Date:** 2026-05-09
**Decision:** Doctor is removed from v1 scope. Inline validation at authoring time replaces it.
**Why:** Doctor was a centerpiece in earlier drafts. Inline validation hits the actual gaps without a separate UI surface.

### D-VELLUM-10 — Vault-rooted cache layout [H-D]
**Date:** 2026-05-10 (cycle 1)
**Decision:** Cache lives at `<vault-root>/.vellum-cache/<docpath>/` — vault-rooted with path-from-vault-root as subfolder hierarchy. Holds `runs.ndjson`, pinned results, `identity.json`.
**Why:** Sibling-to-doc cache pollutes the working tree and can't survive Finder/`git mv` reliably. Vault-rooted plus auto-migration on rename solves both.

### D-VELLUM-11 — 30A/30B milestone split [H-D]
**Date:** 2026-05-10 (cycle 1)
**Decision:** Day-30 milestone splits into Gate 30A (parser + corpus, target ~day 15-18) and Gate 30B (editor end-to-end, target day 30). Editor work does not begin until 30A passes.
**Why:** Original Day-30 milestone bundled 11+ workstreams under one gate. One parser bug failed both criteria; one missing PM node type failed the second. Splitting makes failures visible.

### D-VELLUM-12 — Sidecar auto-migration on rename [H-D]
**Date:** 2026-05-10 (cycle 4)
**Decision:** On open, if sidecar absent at expected path, scan vault cache for `identity.json` whose recorded source-hash matches the file. Migrate. Fall back to cold-state only if no match.
**Why:** Finder rename and `git mv` are common cases. Without migration, every rename drops to cold state — defeats the "warm-start coherence" purpose of the sidecar. Auto-migrate solves the common case without user friction.

### D-VELLUM-13 — Server identity binding [H-R]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** Server entries in `[servers]` are records `{state, bind}` where `bind` is `command:<exec-spec>` | `url:<scheme://...>` | `*`. Trust requires BOTH advertised name AND bind to match. Default `unknown_server = "ask"` (was `block`).
**Why:** Codex flagged that handshake-name alone is spoofable: a malicious server claiming `github` would inherit existing trust. `bind` raises the spoofing bar to also requiring the configured invocation. `unknown_server = ask` (not block) makes the documented "first-call trust prompt" reachable.

### D-VELLUM-14 — IPC types: ts-rs + handwritten Zod [H-R]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** `ts-rs` generates TypeScript types from Rust. Zod schemas at the IPC boundary are handwritten. CI type-tests verify the generated TS shape matches the Zod schema.
**Why:** Original spec said "serde ↔ zod via ts-rs" but `ts-rs` does not generate Zod validators. Either we hand-write Zod or pick a different generator. Hand-writing Zod is cheaper than introducing a second generator; CI tests prevent drift.

### D-VELLUM-15 — v1.0 ships unsigned/ad-hoc-signed [H-D]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** Day-90 v1.0 release uses ad-hoc / developer-signed binaries only. macOS notarization, Windows code-signing certs, and Linux signature distribution are deferred to v1.1. README documents the workaround (right-click → Open on macOS, SmartScreen click-through on Windows).
**Why:** Codex flagged "signed binaries by Day 90" as hiding multi-day operational tracks (Apple Developer Program enrollment + notarization workflow, Windows code-signing cert purchase). Deferring is honest; v1.0 still ships.

### D-VELLUM-16 — Block patch contract written before scaffold [H-R]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** §Block patch contract section in spec defines the IPC shape between PM and Rust before any code is written: BlockPatch struct, BlockEdit enum (PreservedBytes/EditedBytes/SerializeFromTree), edit-source preference rule, merge/split/delete semantics, save flow, error model.
**Why:** Codex flagged that "content and structure unchanged" was not a concrete equality rule — this is exactly where corruption bugs concentrate. Writing the contract before scaffolding prevents the patch contract from being invented ad-hoc.

### D-VELLUM-17 — Save corruption guard [H-R]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** Immediately before `rename(2)`, the save path stat+hashes the current target file and compares against the in-memory base hash. If the on-disk hash differs from base, abort to three-way conflict flow.
**Why:** Codex flagged that watcher events can be missed or delayed — without a stat-before-rename guard, an external write that races the save would be overwritten silently. This is a corruption guard, not a feature.

### D-VELLUM-18 — Cold-state behavior rewrite [H-D]
**Date:** 2026-05-10 (cycle 4, double-confirmed from Codex + Claude -p external review)
**Decision:** Cold-state (sidecar absent, no migration found) preserves raw bytes from the parser. Identity map matters only across edit sessions, not for no-op round-trip byte preservation. Both steady-state and cold-state corpus gates are reachable.
**Why:** Original cycle 1 patch had "loss of sidecar → every block re-serializes" which contradicted the cold-state byte-identical gate. Cross-model convergence between Codex and Claude -p flagged this as Critical. Rewrite separates byte-preservation (which uses parser output) from identity tracking (which uses the sidecar).
**Replaces:** Cycle 1 cold-state rule.

### D-VELLUM-19 — Block partition contract is non-overlapping [H-R]
**Date:** 2026-05-10 (cycle 4, from Codex external review)
**Decision:** Parser produces an ordered, non-overlapping byte-span partition. Top-level blocks each own one preserved-bytes span. Inner blocks (Item, BlockQuote contents) inform the PM tree but do not produce independent spans. CI invariant test enforces.
**Why:** Codex flagged that treating List, Item, BlockQuote as independent block frames risks overlapping byte ranges. Without a non-overlapping partition contract, the parser is not scaffold-ready.

### D-VELLUM-20 — `data` is canonical-JSON regardless of renderer [H-D]
**Date:** 2026-05-10 (cycle 4, from Claude -p external review)
**Decision:** `vellum:result.data` is YAML-quoted, canonical-JSON encoded regardless of `render` mode. `result_hash = blake3(canonical_json(tool_response))`, computed in Rust before any renderer transform. Renderer-specific interpretation, not encoding.
**Why:** Original spec left `data` payload type undefined across `render` modes. Unified encoding makes hashing reproducible and `result_hash` definable.
