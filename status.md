---
project: vellum
updated: 2026-05-10
stage: Design → Develop transition
---

# Vellum — Status

## Right now

**Design stage complete. Develop stage beginning.**

The spec is locked through 4 review cycles (3 internal CPO + 2 external multi-model rounds — Codex implementation lens + Claude -p architectural lens). Critical=0, High=0 at exit. Project artifacts (intent, decisions, BACKLOG, this status) just landed.

**Next move:** scaffold the Cargo workspace + format-preservation corpus via Codex delegation. This is Phase 0 + Gate 30A.

## Session log

### 2026-05-10 — Phase 0 + Gate 30A scaffold

- Scaffolded root project files: Apache 2.0 `LICENSE`, Contributor Covenant 2.1 `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `README.md`, `.gitignore`, Rust toolchain pin, Cargo workspace, and CI.
- Created Rust workspace crates: `vellum-core`, `vellum-app`, and `vellum-corpus`.
- Added `vellum-core::parse` skeleton with `parse()` intentionally left as `todo!()` per Gate 30A sequencing.
- Implemented `parse::partition::verify_partition` plus invariant tests for valid partitions, gaps, overlaps, incomplete coverage, and out-of-bounds ranges.
- Added `BlockPatch`, `BlockEdit`, and `BlockError` types per the locked block patch contract.
- Added `vellum-corpus` runner that walks corpus Markdown files, round-trips through `vellum-core`, and reports PASS/FAIL; current expected failure is the deferred parser `todo!()`.
- Added 67 small corpus Markdown fixtures covering frontmatter variants, tables, fence styles, HTML block classes, comments, footnotes, link refs, lists, lazy continuation, indented code, heading styles, hard breaks, trailing whitespace, BOM, CRLF/mixed line endings, no trailing newline, and Vellum primitive blocks.
- Installed Rust stable and Tauri Linux build prerequisites inside OrbStack dev VM only; CI mirrors the required Ubuntu packages.
- Verification: `cargo build --workspace` passes; `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` pass; `cargo run -p vellum-corpus` fails as expected with 67 parser-todo failures.

### 2026-05-10 — Design review + project structure

- Ran review-loop skill against `vellum-spec-v0.3.md`.
- Internal review: 3 cycles, 16 findings → 13 fixed inline, 3 logged-only.
- External review round 1 — Claude -p Opus 4.7: 13 findings (1 Critical, 7 High, 5 Low).
- External review round 2 — Codex GPT-5: 7 findings (2 Critical, 5 High, 0 Low).
- External review round 3 — Claude -p verification pass: 9 findings (0 Critical, 3 High, 6 Low).
- **Cross-model convergence on cold-state contradiction** (both Codex + Claude -p flagged independently) — strong signal, structural fix applied.
- 3 user-decision flags resolved: Y-H1 (block patch contract subsection — chose new section), Z-H2 (sidecar rename — chose auto-migrate by source-hash), Z-H3 (primitive id pre-save — chose auto-inject on first save).
- Cycle 4 synthesis patch landed: ~100 lines added across §Block identity, §Block patch contract (NEW), §Block boundary reconstruction (partition contract), §Rust core file system (corruption guard), §Primitive schema (field statuses), §`vellum:result` (encoding + result_hash), §Trust (bind field + evaluation order), §Risks (#7 + #8).
- Spec grew 465 → 618 lines. Spec is locked, second sign-off applied.
- Project structure stood up: `intent.md`, `decisions.md` (20 commitments), `BACKLOG.md` (P0 + 30A + 30B + 60 + 90 + v1.5 + cut), `status.md`, this commit.
- `review-findings.md` (229 lines) is the durable artifact of the review.

## Recent decisions

See `decisions.md` for the full log. Highlights from cycle 4:

- D-VELLUM-13: Server identity binding via `{state, bind}` records — anti-spoofing for MCP trust.
- D-VELLUM-14: ts-rs for TS types + handwritten Zod for runtime validation (ts-rs alone doesn't generate Zod).
- D-VELLUM-15: v1.0 ad-hoc/dev-signed only — notarization deferred to v1.1.
- D-VELLUM-16: Block patch contract written before any code touches the IPC.
- D-VELLUM-17: Save corruption guard — stat+hash check immediately before `rename(2)`.
- D-VELLUM-18: Cold-state behavior rewritten — parser preserves raw bytes without needing the identity sidecar.
- D-VELLUM-19: Block partition is non-overlapping by contract; CI invariant test enforces.
- D-VELLUM-20: `data` is canonical-JSON regardless of renderer; `result_hash = blake3(canonical_json(tool_response))`.

## Next steps (priority order)

1. **P0-03 CLAUDE.md + AGENTS.md** — project-local agent context. Authoring this session.
2. **P0-04 `gh repo create jessepike/vellum --public`** — user action. Spec mandates public from commit 1.
3. **30A-01 Cargo workspace scaffold** — delegate to Codex via codex-delegate.
4. **30A-02 format-preservation corpus** — delegate to Codex; 50+ test files.
5. **30A-04 vellum-core::parse module** — partition-contract parser.
6. **30A-05 partition invariant CI test.**

## Blockers / open questions

- Repo init: `gh repo create` is a user action — I (CPO) can't push to GitHub on your behalf.
- All other 30A items are Codex-delegatable. Scaffold runs autonomously once kicked off.

## Risks watched

- **R7 — server identity binding is best-effort.** `bind` raises the spoofing bar but `bind: *` opts out; wildcard server patterns matching too broadly are also exposed. Manifest discourages `bind: *` outside `[defaults]`.
- **R8 — v1.0 unsigned/ad-hoc-signed.** First-launch warnings on macOS + Windows. README documents workaround.
- **Layer A timeline:** Layer A (solo agentic build), 0.1–0.3× of 2022 baseline. Gate 30A is the load-bearing wall — its honesty paces everything downstream. No calendar commit; exit-condition-gated.

## Files in this project (right now)

- `vellum-spec-v0.3.md` — locked design spec (618 lines, sign-off applied).
- `review-findings.md` — review-loop durable artifact (229 lines).
- `intent.md` — destination-only product intent.
- `decisions.md` — 20 commitments with confidence tags.
- `BACKLOG.md` — Phase 0 + 30A + 30B + 60 + 90 + v1.5 + cut.
- `status.md` — this file.
- `data/` — pre-existing (chroma, kb.db); orthogonal to vellum, not Vellum's data dir.

No source code yet. By design — the spec is the design artifact; code begins at 30A-01.
