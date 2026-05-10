---
project: vellum
updated: 2026-05-10
stage: Design → Develop transition
---

# Vellum — Status

## Right now

**Gate 30A is fully closed. Develop stage active; Gate 30B-01a minimal UI scaffold proves the Tauri IPC roundtrip path.**

The spec is locked through 4 review cycles (3 internal CPO + 2 external multi-model rounds — Codex implementation lens + Claude -p architectural lens). Critical=0, High=0 at exit. Project artifacts (intent, decisions, BACKLOG, this status) just landed.

**Next move:** implement Gate 30B-01b CodeMirror source view, then Gate 30B-01c ProseMirror custom schema, as separate reviewable slices.

## Session log

### 2026-05-10 — Gate 30B-01a minimal UI IPC roundtrip

- Added `ui/` React 19 + Vite + TypeScript scaffold with a pasteable/file-load Markdown textarea, Parse button, and pretty-printed `Block[]` JSON output.
- Added `ui/src/ipc.ts` wrapper for `invoke("parse_document", { source })`, validating returned `Block[]` with handwritten Zod schemas per D-VELLUM-14.
- Added handwritten Zod schemas for `BlockKind`, `Block`, byte ranges, and string parse errors. No `ts-rs` derives are wired yet.
- Wired Tauri config to Vite dev server at `http://localhost:1420`, frontend dist at `../../ui/dist`, and pnpm commands from the app crate into `ui/`.
- Updated Gate 30B backlog split: 30B-01a done, 30B-01b CodeMirror source view, 30B-01c ProseMirror schema. No CodeMirror or ProseMirror dependencies were added in this slice.
- Verification: `pnpm install`, `pnpm build`, `cd crates/vellum-app && cd ../../ui && pnpm --filter . build`, `cargo build --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `cargo test --workspace`, and `cargo run -p vellum-corpus` pass. Corpus remains 67/67.

### 2026-05-10 — Gate 30B-00 BlockPatch save flow

- Implemented root `vellum_core::save(source, patches)` with `SaveError` for partition invariant failures, patch validation failures, missing preserved byte ranges, and unimplemented serializer requests.
- Save now resolves ordered `BlockPatch` sequences by emitting preserved source slices, emitting `EditedBytes` verbatim, or rejecting `SerializeFromTree` for every block kind until the rendered-view structured representation is specified.
- Added integration tests in `crates/vellum-core/tests/save_integration.rs` covering byte-identical preserved saves against representative corpus files, paragraph edit, insertion, deletion, merge, missing original bytes, pre-populated patch error, overlapping input ranges, and serializer rejection.
- Judgment call: omitted `Some` ranges are treated as deliberate deletion/merge gaps and are allowed when ranges remain ordered and non-overlapping; otherwise the requested delete and merge patch sequences could not pass.
- Verification: `cargo build --workspace`, `cargo test --workspace` (41 tests), `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, and `cargo run -p vellum-corpus` pass. Corpus remains 67/67.

### 2026-05-10 — Gate 30A-08 watcher and conflict marker detector

- Implemented `vellum_core::watch::watch_vault(vault_root, callback)` on `notify` with recursive vault watching, `.md` filtering, `.vellum-tmp-*` filtering, `.vellum-cache/` subtree filtering, 50ms modify debounce, changed-file blake3 hashing, and best-effort warning logs for backend or hashing errors.
- Added `WatchEvent::{Changed, Created, Removed, Renamed}` and opaque `WatchHandle`; dropping the handle drops the underlying `RecommendedWatcher` and joins the worker thread.
- Implemented `vellum_core::fs::has_conflict_markers(source)` for Git-style conflict markers at column 0, ignoring markers inside fenced code blocks and treating `=======` after non-empty text as a Setext H1 underline.
- Updated Gate 30A backlog rows to done, including stale already-landed scaffold/parser/corpus/CI rows. Gate 30A is now fully closed.
- Verification: focused `cargo test -p vellum-core` passes (29 core unit tests).

### 2026-05-10 — Gate 30A filesystem, sidecar, reaper, and Tauri scaffold

- Implemented `vellum_core::fs::atomic_write(target, contents, base_hash)` with same-directory `<doc>.vellum-tmp-<pid>-<short_uuid>` writes, blake3 base-hash precondition, atomic `rename`, target-directory and parent-missing errors, and best-effort tmpfile cleanup on failure.
- Implemented `vellum_core::fs::reap_stale_tmpfiles(vault_root)` with recursive `*.vellum-tmp-*-*` discovery and conservative pid liveness checks (`/proc/<pid>` on Linux, `kill(pid, 0)` elsewhere).
- Replaced the ID stub with `BlockId = Uuid` and `fresh()`.
- Added `vellum_core::sidecar` with `IdentityMap`, `BlockIdentity`, vault-rooted `.vellum-cache/<docpath>/identity.json` pathing, save, load, and source-hash migration for renamed/moved docs.
- Added minimal Tauri 2 shell in `vellum-app`: `parse_document(source: String)` IPC command, `generate_context!()`, `tauri.conf.json`, `build.rs`, and a placeholder icon required by Tauri codegen. No `ui/` scaffold and no `ts-rs` derives yet.
- Updated Gate 30A backlog rows 30A-07, 30A-09, 30A-10, and 30A-12 to done.
- Verification in OrbStack dev VM: `cargo build -p vellum-app` passes; `cargo build --workspace` passes; `cargo test --workspace` passes (21 core tests, 0 app tests, 0 corpus unit tests); `cargo clippy --workspace -- -D warnings` passes; `cargo fmt --check` passes; `cargo run -p vellum-corpus` passes 67/67.

### 2026-05-10 — Gate 30A-04 parser body

- Implemented `parse::parse(source: &str) -> Result<Vec<Block>, ParseError>` in `crates/vellum-core/src/parse/mod.rs`.
- Parser now detects YAML/TOML/JSON frontmatter as an opaque first block, scans top-level link reference definition lines, walks `pulldown-cmark` 0.13 `OffsetIter` for top-level block frames, maps Vellum primitive fences to `VellumLiveQuery` / `VellumResult`, stitches trailing whitespace into the preceding block, and validates with `parse::partition::verify_partition` before returning.
- Added `ParseError::PartitionInvariant` so parser callers get an explicit invariant failure instead of a panic or unchecked bad partition.
- Verification: `cargo run -p vellum-corpus` passes 67/67; `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` pass in OrbStack dev VM.

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

1. **30A-05 partition invariant CI test.**
2. **P0-04 `gh repo create jessepike/vellum --public`** — user action. Spec mandates public from commit 1.
3. **Editor-facing parse surface** — expose block metadata needed by the UI once Gate 30A is fully closed.
4. **30B planning** — ProseMirror/CodeMirror shell after parser/corpus gate is stable.

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
