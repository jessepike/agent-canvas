# Contributing to Vellum

Vellum is built around one non-negotiable contract: user Markdown is a durable artifact, not an editor-owned serialization format. The source file must stay plain, inspectable, and byte-stable unless the user explicitly changes content.

The design source is [`vellum-spec-v0.3.md`](vellum-spec-v0.3.md). If implementation and preference disagree, follow the locked spec or open a decision before changing architecture.

## Architecture

The Rust core owns durability: parsing, block byte ranges, filesystem safety, hashing, cache layout, primitive runtime, and future MCP integration. The UI is a thin shell. ProseMirror will own live rendered editing, CodeMirror will own source view, and Rust will not maintain a competing live editing tree.

The current scaffold is Gate 30A: parser API, partition invariant, and the format-preservation corpus. UI, MCP, auth, and renderer work come later.

## UI setup

The UI is in `ui/` — React + Vite + TypeScript.

Install: `cd ui && pnpm install`.
Dev server: `pnpm dev` (Vite hot reload on http://localhost:1420).
Tauri dev mode (UI + Rust together): from project root, `cargo tauri dev`.
Build: `pnpm build` → outputs to `ui/dist/`.

## Format Preservation

Every parsed top-level block carries a byte range into the original file. Untouched blocks are emitted as their preserved raw bytes. The parser must produce an ordered, non-overlapping partition of the entire file: no gaps, no overlaps, no nested top-level spans.

The corpus under `crates/vellum-corpus/corpus/` is not sample content to be cleaned up. It is test evidence. Whitespace, fence length, line endings, frontmatter delimiters, comments, hard breaks, and trailing bytes are deliberate.

Any feature proposal that pretty-prints the file on save gets a polite no.

## Development Rules

- Do not implement architectural changes without a decision in `decisions.md`.
- Do not normalize Markdown as a side effect of parsing or saving.
- Add corpus coverage before relying on a Markdown edge case.
- Keep changes scoped to the current gate.
- Run `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test --workspace` before proposing completion once the toolchain is available.
