# Vellum — Codex Context

This file is project-local context for Codex CLI sessions. It mirrors `CLAUDE.md` with the same priority order but is the file Codex reads on entry.

## Quick start

Read in order: `intent.md` → `vellum-spec-v0.3.md` → `decisions.md` → `BACKLOG.md` → `status.md`.

The spec is **locked**. Implementation work uses the spec as source of truth. Any architectural change requires opening a decision in `decisions.md` first.

## Current task class

**Phase 0 scaffold + Gate 30A (parser + format-preservation corpus).** This is the load-bearing wall — editor work does not start until corpus passes byte-identical at both steady-state and cold-state gates.

## Stack pins (D-VELLUM-1, 14, 16)

- Rust core: `tauri 2.x`, `tokio 1.x`, `serde 1.x`, `notify 6.x`, `rmcp` (latest), `pulldown-cmark 0.13+` (for `OffsetIter`), `blake3 1.x`, `tracing 0.1`, `thiserror 1.x`, `time 0.3`, `toml 0.8+`, `uuid 1.x`.
- UI: TypeScript + React + Vite + ProseMirror (NOT Tiptap) + CodeMirror 6.
- IPC types: `ts-rs` for TS types from Rust + handwritten Zod schemas at IPC boundary. CI type-tests reconcile.

## Workspace layout (target)

```
vellum/
├── crates/
│   ├── vellum-core/      Rust core (parser, runtime, fs, MCP, hashing)
│   ├── vellum-app/       Tauri shell entry
│   └── vellum-corpus/    Format preservation test corpus + runner
├── ui/                   React + PM + CM6
├── docs/                 architecture.md, format-preservation.md, trust.md
├── .github/workflows/    CI
├── Cargo.toml            workspace
├── package.json
├── README.md
├── LICENSE
├── CODE_OF_CONDUCT.md
└── CONTRIBUTING.md
```

## Hard rules (do not violate)

1. **Source preservation is byte-level.** Never pretty-print, normalize, or rewrite user Markdown by default. The format-preservation corpus is the gate.
2. **ProseMirror is authoritative for live editing.** Rust does NOT maintain a parallel editing tree.
3. **Block partition is non-overlapping.** CI invariant: `Σ block.byte_range == 0..file.len()`, no gaps, no overlaps.
4. **Save guard.** Stat+hash the target before `rename(2)`; abort if on-disk hash differs from base.
5. **Atomic write tmpfile naming:** `<doc>.vellum-tmp-<pid>-<short_uuid>` in the SAME directory as the target (not system tmpdir).
6. **Cache lives at `<vault-root>/.vellum-cache/<docpath>/`** — vault-rooted with path-as-subfolder. Not sibling-to-doc. Not basename-only.
7. **Trust is one file: `~/.vellum/trust.toml`.** Three states (`trusted`/`ask`/`block`) per tool/server. `bind` field on server entries for anti-spoofing.

## What ships in v1.0 (Day 90)

- Format-preservation editor core with corpus passing
- ProseMirror + CodeMirror UI with toggle-time sync
- `vellum:live-query` primitive end-to-end
- GitHub MCP server (read-only) integration with token auth
- Trust toml + settings panel
- Six built-in renderers (`table`, `list`, `card`, `json`, `markdown`, `metric`)
- Manual freeze command + `vellum:result` block + Evidence State badges
- Append-only `runs.ndjson`
- Conflict-safe save (three-pane diff)
- Ad-hoc / dev-signed binaries for macOS (Intel+ARM), Windows, Linux

## What does NOT ship in v1.0

- Notarization, code-signing certs (v1.1)
- OAuth flows for MCP (v1.5)
- `vellum:agent`, `vellum:transform`, `vellum:include` (v1.5)
- iOS reader (v1.5)
- Tantivy search UI (v1.5)
- Evidence bundle export (v1.5)
- `chart` renderer (v1.5)
- Plugin marketplace (never)
- Telemetry (opt-in only, probably never)
- Cloud account requirement (never)
- AI sidebar (never — primitives are inline)

## Commit conventions

- Conventional Commits: `feat(scope): description`, `fix(scope): ...`, `chore(scope): ...`.
- Atomic commits at each completed unit. Never bundle unrelated changes.
- Commit body cites the spec section or decision ID being implemented (`Implements D-VELLUM-19 partition contract` / `Per spec §Block patch contract`).
- Co-author tag for Codex contributions:
  `Co-Authored-By: GPT-5 (Codex) <noreply@openai.com>`

## When to escalate

- Architectural ambiguity not covered by the spec → stop, ask the user to open a decision in `decisions.md`.
- Library version mismatch between spec pins and reality → flag; do not silently swap.
- Corpus regression → blocker; stop and surface.
- Format-preservation contract violation → blocker; stop and surface.

## When to push forward without asking

- Implementation choices INSIDE the spec's specified algorithm (variable naming, internal helper functions, error message wording, file organization within a crate).
- Test scaffolding and CI configuration.
- Documentation comments and rustdoc.
- Lint / formatting / clippy fixes.

## Format preservation corpus — examples to include

The corpus must cover (at minimum):
- Frontmatter variants: YAML (`---`), TOML (`+++`), JSON (rare but exists)
- Table alignment: left, right, center, default, mixed
- Fence lengths: 3, 4, 5+ backticks; tildes; info strings with extras
- HTML blocks: HTML5 block-level, CommonMark Type-1 through Type-7
- HTML comments inline and between blocks
- Footnotes: `[^id]:` definitions and `[^id]` references
- Link reference definitions (must NOT inline into paragraphs that consume them)
- Mixed list styles: tight, loose, `-` vs `*` vs `+`, ordered with `.` vs `)`, nested
- Lazy continuation lines inside lists and blockquotes
- Indented code blocks adjacent to lists
- Setext (`===` / `---`) vs ATX (`#`) headings
- Hard line breaks (`  \n` and `\\\n`)
- Trailing whitespace (preserved)
- BOM at start of file
- CRLF vs LF line endings
- Trailing newline vs no trailing newline at EOF

Each test file is named for its case (e.g., `frontmatter-toml.md`, `lazy-continuation-list.md`). The corpus runner opens each, saves, asserts byte-identical, fails loudly with a diff on regression.
