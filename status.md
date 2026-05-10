---
project: vellum
updated: 2026-05-10
stage: Design → Develop transition
---

# Vellum — Status

## Right now

**Gate 30A is fully closed. Develop stage active; Gate 30B-save is implemented: source-only open/save uses Tauri dialogs, raw Markdown writes go through `atomic_write` with base-hash corruption guard, and conflict saves are blocked with a reload banner.**

The spec is locked through 4 review cycles (3 internal CPO + 2 external multi-model rounds — Codex implementation lens + Claude -p architectural lens). Critical=0, High=0 at exit. Project artifacts (intent, decisions, BACKLOG, this status) just landed.

**Next move:** implement Gate 30B-04 toggle-time bidirectional sync. Keep it separate from external-change diff UI / three-way merge resolution.

## Session log

### 2026-05-10 — Gate 30B-save Open / Save / Cmd+S round-trip

- Extended `vellum_core::fs::atomic_write` to return the blake3 hash of the bytes just written while preserving the same-directory tmpfile and base-hash conflict guard.
- Added ts-rs exported `OpenDocument` and `WriteResult` IPC structs.
- Added Tauri commands `open_document(doc_path)` and `write_document(doc_path, source, base_hash)` with regular-file validation, conflict-marker detection on open, and `CONFLICT:` string prefix for base-hash mismatch until the typed 30B-05 merge error channel lands.
- Replaced the browser file input with Tauri dialog plugin open/save flows. UI now tracks `docPath`, `baseHash`, `dirty`, supports Save As for untitled buffers, prompts before opening over dirty edits, and shows filename plus dirty/saved marker.
- Added Cmd/Ctrl+O and Cmd/Ctrl+S handling both at document level and inside CodeMirror via a high-precedence keymap. CodeMirror external value updates are suppressed from dirty tracking so opening/reloading does not mark a document dirty.
- Added conflict banner text: "File changed on disk since open. Save aborted — reload or open three-way merge." with a "Reload from disk" action.
- Installed Node/npm/pnpm inside the OrbStack dev VM to satisfy the project isolation rule for UI dependency installs; `pnpm` pinned to 9.15.9 because Ubuntu's Node 18 cannot run pnpm 11.
- Verification: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --all --check`, `cargo run -p vellum-corpus`, and `pnpm install --force && pnpm run build` pass. Corpus remains 67/67; Vite reports only the chunk-size warning.

### 2026-05-10 — Gate 30B-ipc Tauri IPC surface + ts-rs exports

- Added ts-rs exports for Rust block, patch, block ID, byte range, and sidecar identity types. `cargo test --workspace` now regenerates TypeScript bindings into `ui/src/types/generated/`.
- Introduced concrete Rust `ByteRange { start, end }` and a transparent `BlockId` newtype over `Uuid` so the IPC contract has exportable Rust types while preserving existing JSON shape.
- Added Tauri commands: `save_document(source, patches)`, `load_sidecar(doc_path)`, and `save_sidecar(doc_path, map)`. Current sidecar IPC uses absolute doc paths and treats the document parent as the temporary vault root until app-level vault state exists.
- Replaced stale handwritten UI block types with Zod schemas checked against generated ts-rs types. The boundary remains snake_case to match serde output and the locked Rust/spec field names.
- Replaced `ui/src/pm/sidecarBridge.ts` stubs with thin re-exports from `ui/src/ipc.ts`.
- Removed the stale UI assumption that parsed `Block` includes optional `id`; PM-decorated IDs still belong to the PM plugin until a richer parser/identity matching slice lands.
- Verification: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `cargo run -p vellum-corpus`, and `pnpm install && pnpm run build` pass. Corpus remains 67/67; Vite reports only the existing chunk-size warning.

### 2026-05-10 — Gate 30B-02 PM-decorated stable block IDs

- Added `ui/src/pm/blockIdsPlugin.ts`, a ProseMirror plugin that assigns browser-native UUIDs to unidentified non-primitive top-level nodes, leaves primitive IDs canonical when present, exposes `getBlockIds(state)`, and decorates top-level blocks with `data-block-id`.
- Added `ui/src/pm/sidecarBridge.ts` as a stub for the follow-up Tauri IPC commands `load_sidecar` and `save_sidecar`; persistence across sessions remains out of scope for this slice.
- Updated the PM schema so top-level editable block node types carry optional `id` attrs. `frontmatter` remains ID-less.
- Updated `blocksToDoc` and the `Block` Zod schema to propagate optional `Block.id` when future parser/IPC payloads expose it. Current Rust `Block` still exposes only `kind`, `byte_range`, and `raw_source`, so the plugin assigns generated IDs on mount for current non-primitives.
- Updated primitive node views to preserve `data-block-id` on node-view DOM when an `id` attr is present.
- Verification: `pnpm install` passes; `pnpm run build` passes (`tsc --noEmit && vite build`). Vite reports only the existing chunk-size warning.
- Browser DOM verification was attempted but blocked by unavailable browser automation / sandboxed macOS app launch. Production bundle inspection confirms the runtime `data-block-id` decoration path is emitted.

### 2026-05-10 — Gate 30B-01c ProseMirror schema + rendered view

- Added `ui/src/pm/schema.ts` with a custom ProseMirror schema for Vellum block nodes, primitive atoms, frontmatter, and basic marks. No `prosemirror-schema-basic` dependency.
- Added `ui/src/pm/nodeviews.ts` with placeholder node views for `vellum:live-query`, `vellum:result`, and collapsed frontmatter.
- Added `ui/src/components/RenderedView.tsx`, a read-only React component that mounts/disposes a ProseMirror `EditorView` and rebuilds the PM doc from current parser `Block[]`.
- Updated `ui/src/App.tsx` and `ui/src/styles.css` to show SourceView on top and RenderedView below, both driven by the Parse button.
- Added ProseMirror deps: `prosemirror-model`, `prosemirror-state`, `prosemirror-view`, `prosemirror-keymap`, and `prosemirror-commands`.
- Verification: `pnpm install prosemirror-model prosemirror-state prosemirror-view prosemirror-keymap prosemirror-commands` passes; `pnpm run build` passes (`tsc --noEmit && vite build`). Vite reports only the bundle-size warning.
- Scope intentionally deferred: PM-decorated block IDs (30B-02), editable rendered view, and source/rendered patch sync (30B-04). Current parser `Block[]` lacks text content and primitive YAML attrs, so rendered nodes use placeholders until a later parser/IPC slice exposes richer block payloads.

### 2026-05-10 — Gate 30B-01b CodeMirror source view

- Added `ui/src/components/SourceView.tsx`, a small React functional component that mounts a CodeMirror 6 `EditorView`, enables Markdown grammar, line wrapping, default light UI, per-transaction `onChange`, unmount disposal, and external prop-to-doc resync for later file watcher integration.
- Replaced the scaffold textarea in `ui/src/App.tsx` with `SourceView` while preserving file load, Parse button, and pretty-printed parsed `Block[]` output.
- Updated `ui/src/styles.css` so the source editor fills roughly half the viewport and the result pane sits below as a bounded scrollable region.
- Added CM6 dependencies: `codemirror`, `@codemirror/lang-markdown`, `@codemirror/state`, and `@codemirror/view`.
- Verification: `pnpm install` passes; `pnpm run build` passes (`tsc --noEmit && vite build`). No lint script exists in `ui/package.json`.

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

1. **30B-04 Toggle-time bidirectional sync** between source view and rendered view, now using the real `save_document` IPC wrapper instead of a future placeholder.
2. **30B-05 External-change diff prompt UI** — consumes the watcher events from 30A-08.
3. **30B-06 Split-view layout + 30B-07 themes.**
4. **Parser/UI identity follow-up:** richer block payload IDs or sidecar matching rules so parsed blocks can be reconciled with PM node IDs across sessions.

## Blockers / open questions

- None blocking. Repo public at github.com/jessepike/vellum, 7 commits on main, CI green.
- 30B-04 follow-up: decide the exact UI matching contract for sidecar `BlockIdentity { id, byte_range_start, kind }`. Rust IPC now exposes sidecar load/save, but the UI still needs the rendered/source sync slice to match non-primitive sidecar entries by byte offset.
- UI tooling now exists inside OrbStack dev VM (`node` 18.19.1, `pnpm` 9.15.9). Keep package installs and UI builds there, not on the host.

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
