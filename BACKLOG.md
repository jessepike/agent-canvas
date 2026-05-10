---
project: vellum
updated: 2026-05-10
---

# Vellum — Backlog

Status: `todo` / `in-progress` / `blocked` / `done` / `cut`.
Confidence: `[H/M/L]` × `[R/D/G/F]`.

---

## Phase 0 — Project scaffold (before Gate 30A code)

| ID | Status | Item | Confidence | Notes |
|----|--------|------|-----------|-------|
| P0-01 | done | Spec v0.3 locked through 4 review cycles (3 internal + 2 external rounds) | [H-D] | Critical=0, High=0 at exit |
| P0-02 | done | intent.md, decisions.md, BACKLOG.md, status.md authored | [H-D] | This commit |
| P0-03 | todo | CLAUDE.md + AGENTS.md for project-local context | [H-D] | Authored alongside this batch |
| P0-04 | todo | `gh repo create jessepike/vellum --public --apache-2.0` | [H-F] | User action; D-VELLUM-3 |
| P0-05 | todo | LICENSE, CODE_OF_CONDUCT.md, CONTRIBUTING.md files | [H-D] | Standard set + format-preservation-first contributing philosophy |
| P0-06 | todo | `.gitignore` for Rust + Node + Tauri + `.vellum-cache/` | [H-D] | |
| P0-07 | todo | Initial commit + first push | [H-F] | User action |

## Gate 30A — Parser + corpus (load-bearing wall)

| ID | Status | Item | Confidence | Notes |
|----|--------|------|-----------|-------|
| 30A-01 | done | Cargo workspace scaffold: `crates/vellum-core`, `crates/vellum-app`, `crates/vellum-corpus`, top-level `Cargo.toml` | [H-D] | Implemented |
| 30A-02 | done | `vellum-corpus` crate: directory of 50+ `.md` test files covering hard cases per spec §Block boundary reconstruction | [H-D] | 67 corpus fixtures |
| 30A-03 | done | `vellum-corpus` runner: open every corpus file, save, assert byte-identical | [H-D] | 67/67 PASS |
| 30A-04 | done | `vellum-core::parse` module: block-boundary reconstruction on top of `pulldown-cmark` `OffsetIter` | [H-R] | D-VELLUM-19 partition contract; corpus passing |
| 30A-05 | done | Partition invariant test: `Σ block.byte_range == 0..file.len()`, no gaps, no overlaps | [H-R] | Implemented in `parse::partition` unit tests |
| 30A-06 | done | `BlockPatch` struct + `BlockEdit` enum per D-VELLUM-16 §Block patch contract | [H-R] | Rust-side types implemented |
| 30A-07 | done | Atomic write path: same-volume tmpfile (`<doc>.vellum-tmp-<pid>-<short_uuid>`), `rename(2)`, stat+hash precondition (D-VELLUM-17) | [H-R] | Implemented in `vellum-core::fs`; unit-covered |
| 30A-08 | done | `notify`-backed file watcher; conflict-marker detection on open | [H-D] | Implemented in `vellum-core::watch` and `vellum-core::fs`; unit-covered |
| 30A-09 | done | Sidecar identity map: `<vault-root>/.vellum-cache/<docpath>/identity.json` with auto-migration on rename (D-VELLUM-12) | [H-R] | Implemented in `vellum-core::sidecar`; unit-covered |
| 30A-10 | done | Tauri 2.x scaffold; IPC types via `ts-rs` + handwritten Zod schemas (D-VELLUM-14) | [H-D] | Minimal Tauri shell + `parse_document`; `ts-rs` dependency only, no derives yet |
| 30A-11 | done | GitHub Actions CI: cargo test, vitest, format-preservation corpus runner on every commit | [H-D] | Implemented in `.github/workflows/ci.yml` |
| 30A-12 | done | Tmpfile reaper on vault open: kill stale `*.vellum-tmp-*` whose pid is not a live Vellum process | [M-D] | Implemented in `vellum-core::fs`; unit-covered |

**Gate 30A exit criterion:** Format preservation corpus passes byte-identical at BOTH the steady-state (sidecar present) and cold-state (sidecar absent) gates. CI green.

## Gate 30B — Editor end-to-end

| ID | Status | Item | Confidence | Notes |
|----|--------|------|-----------|-------|
| 30B-00 | done | BlockPatch save flow in Rust (D-VELLUM-16, edit-preserving save) | [H-R] | Tested via synthetic patch sequences against corpus |
| 30B-01 | done | 30B-01a: minimal React+Vite+TS scaffold + IPC roundtrip | [H-D] | Textarea + `parse_document` invoke + Zod Block[] validation |
| 30B-01b | done | CodeMirror 6 source view | [H-D] | Was part of 30B-03; renumbered for reviewable delegation |
| 30B-01c | done | ProseMirror custom schema with primitive node types | [H-D] | Read-only rendered view; no block IDs or source patch sync yet |
| 30B-02 | done | PM-decorated stable block IDs via PM plugin/metadata | [H-D] | D-VELLUM-6 |
| 30B-03 | cut | CodeMirror 6 source view with Markdown grammar | [H-D] | Replaced by 30B-01b |
| 30B-04 | todo | Toggle-time bidirectional sync between source view and rendered view | [H-D] | NOT per-keystroke |
| 30B-05 | todo | External-change diff prompt UI (three-pane: in-memory / on-disk / base) | [H-D] | |
| 30B-06 | todo | Split-view default layout; rendered-only and source-only modes; per-doc preference | [M-D] | |
| 30B-07 | todo | Vellum Light + Vellum Dark themes; `~/.vellum/theme.css` override | [M-D] | |

**Gate 30B exit criterion:** Open and edit a 2MB doc with mixed primitive and prose blocks, save, reopen — file unchanged except where user typed; corpus still passes after edit round-trip.

## Days 31–60 — `vellum:live-query` end-to-end

| ID | Status | Item | Confidence | Notes |
|----|--------|------|-----------|-------|
| 60-01 | todo | Versioned primitive schema parser + validator | [H-D] | |
| 60-02 | todo | Auto-injection of `id` (UUID-derived) and `created_at` on first save if absent | [H-D] | D-VELLUM-6 lifecycle |
| 60-03 | todo | Inline validation: duplicate id, unknown tool, invalid YAML, orphan `vellum:result`, conflict markers | [H-D] | |
| 60-04 | todo | MCP client integration via `rmcp` crate | [H-R] | |
| 60-05 | todo | GitHub MCP server (read-only) integration; token auth via OS keychain | [H-R] | D-VELLUM-8 |
| 60-06 | todo | `~/.vellum/trust.toml` parser + writer + settings panel | [H-D] | D-VELLUM-7, 13 |
| 60-07 | todo | Trust evaluation order: server gate → tool gate → defaults (D-VELLUM-13) | [H-R] | |
| 60-08 | todo | `bind` field anti-spoofing for server identity (D-VELLUM-13) | [H-R] | |
| 60-09 | todo | First-call trust prompts with rationale (tool name, args, persistent toggle) | [H-D] | |
| 60-10 | todo | Sidecar cache; `transient` + `pinned` result_policy | [H-D] | |
| 60-11 | todo | Built-in renderers: `table`, `list`, `card`, `json`, `markdown`, `metric` | [H-D] | |
| 60-12 | todo | `cache` duration parsing (Go-style: 60s, 5m, 1h, 24h); 0s + absent edge cases | [H-D] | |
| 60-13 | todo | `content_hash` canonical computation: `blake3(version \|\| tool \|\| canonical_yaml(args))` | [H-D] | Persisted = cached echo; live = authoritative |

**Days 31-60 exit criterion:** Author a doc with three live-query primitives against GitHub MCP, open, run, render, edit, save, reopen, refresh. No file corruption. Corpus still passes.

## Days 61–90 — Honest history + ship

| ID | Status | Item | Confidence | Notes |
|----|--------|------|-----------|-------|
| 90-01 | todo | Append-only `runs.ndjson` with content/args/result hashes | [H-D] | |
| 90-02 | todo | Evidence State badges per primitive: Live / Cached / Frozen / Changed-since-frozen / Broken / Untrusted | [H-D] | |
| 90-03 | todo | "Show recipe" toggle on every rendered primitive | [H-D] | |
| 90-04 | todo | Manual freeze command — writes adjacent `vellum:result` block, manual-freeze lifecycle | [H-D] | Unavailable when `result_policy: inline_snapshot` |
| 90-05 | todo | `inline_snapshot` result policy: overwrites `vellum:result` on every successful run | [H-D] | |
| 90-06 | todo | `result_hash = blake3(canonical_json(tool_response))` (D-VELLUM-20) | [H-R] | |
| 90-07 | todo | Frozen-vs-refreshed diff (text + JSON) | [H-D] | |
| 90-08 | todo | Conflict-safe save: three-pane diff on external change (uses corruption guard from 30A-07) | [H-D] | |
| 90-09 | todo | Settings panel: trust config, theme, cache management | [M-D] | |
| 90-10 | todo | README, CONTRIBUTING, format-preservation regression issue template | [H-D] | |
| 90-11 | todo | v1.0 tagged release — ad-hoc/dev-signed binaries for macOS (Intel+ARM), Windows, Linux (D-VELLUM-15) | [H-D] | Notarization deferred to v1.1 |

**Days 61-90 exit criterion:** I am using Vellum daily as my primary Markdown editor. v1.0 is tagged.

---

## v1.5 (post-90-day, priority order)

| ID | Status | Item | Confidence |
|----|--------|------|-----------|
| v1.5-01 | todo | iOS reader — SwiftUI app, iCloud Drive vault, read + light edit (no primitive execution) | [M-D] |
| v1.5-02 | todo | `vellum:agent` primitive — inline LLM calls with streaming | [M-D] |
| v1.5-03 | todo | `vellum:transform` primitive — typed declarative ops only (no DSL) | [M-D] |
| v1.5-04 | todo | `vellum:include` primitive — embed another doc, recursion-guarded | [M-D] |
| v1.5-05 | todo | Tantivy global search across the vault | [M-D] |
| v1.5-06 | todo | Evidence bundle export — zipped `.vellum-evidence/` with manifests + frozen renders | [M-D] |
| v1.5-07 | todo | `chart` renderer via Chart.js | [L-D] |
| v1.5-08 | todo | macOS notarization workflow + Apple Developer Program enrollment | [M-D] |
| v1.5-09 | todo | Windows code-signing certificate + workflow | [M-D] |
| v1.5-10 | todo | OAuth flows / device codes / refresh-token rotation for MCP servers | [M-D] |

## Cut from v1 (named explicitly so they stay cut)

- Vault doctor (D-VELLUM-9; inline validation replaces)
- Per-doc trust grant matrix
- Dataflow-grant UI separate from tool grant
- Renderer plugin manifest (contract is internal-only in v1)
- Evidence bundle export (deferred to v1.5)
- Tantivy search UI (deferred to v1.5)
- Show-as-of-run-X historical render
- Structural table diff
- Plugin model artifacts
- Custom binary format
- Real-time collaboration (CRDT, multiplayer)
- Plugin marketplace
- Telemetry by default
- Cloud account requirement
- AI sidebar

## Open questions tracked

- Whether `pulldown-cmark` `OffsetIter` is sufficient or if `markdown-rs` fallback fires. **Resolves at:** Gate 30A.
- Whether ProseMirror ↔ block-source patch model holds under heavy edit-save cycles. **Resolves at:** Gate 30B + first month of daily use.
- Whether GitHub MCP server's token-only auth is enough for v1 daily use. **Resolves at:** Day 60 milestone.
