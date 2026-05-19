---
project: vellum
status: locked
version: 2.0
updated: 2026-05-19
supersedes: 1.0
---

# Vellum — Intent

## Outcome

A low-friction local workbench for reading, lightly editing, and round-tripping the artifacts my LLM agents produce — Markdown today, HTML increasingly — on Mac, with iOS to follow once daily-utility is proven. Built for one person — me — to use every day. Open source from day one. If anyone else likes it, that's a bonus.

## Why It Matters

The way I work has shifted. I'm no longer the writer of most documents that pass through my hands. My agents — Claude Code, Codex, and the orchestrator persona stack — are producing dozens of artifacts a week: specs, plans, brainstorms, PR explainers, design explorations, reports, status writeups. I'm the reviewer, the redirector, and the merger of those outputs. My job is to consume them fluidly, react, and round-trip changes back to the agent that produced them.

The tools today fail at this in different ways:

- **Native macOS readers** (Quick Look, Marked 2, Preview) are read-only and Markdown-biased. HTML support is browser-territory.
- **Browsers** render HTML well but can't edit, can't save back, can't sync across devices.
- **Code editors** (VS Code, Zed) handle both formats but are heavyweight, code-oriented, and have no roundtrip story.
- **Markdown apps** (iA Writer, Bear, Obsidian, Typora) are writer-tools, not artifact-consumption tools. They don't handle HTML at all.
- **Nothing** treats LLM-produced artifacts as a stream to be reviewed, lightly tweaked, and routed back to the agent that wrote them.

HTML is becoming the right output format for agents — higher information density, richer visual layout, embeddable diagrams, mobile-responsive, interactive widgets, share-via-link, joyful to make. Markdown is the current default but the trajectory is clear (raw text → markdown → HTML → eventually interactive simulations). A tool built today should handle Markdown well and HTML at least as well, and not bet on Markdown being the durable format.

Vellum exists to be the missing tool: fast to open, faithful to the source, friendly to both Markdown and HTML, and built for the loop between me and my agents.

## Problem I'm Pointed At

A daily-driver artifact workbench where:

- **Opening a file is sub-second** — the most common interaction is read, so ingestion has to feel weightless.
- **The file stays plain** — `.md` is `.md`, `.html` is `.html`, no proprietary container. The agent that wrote it or the next agent that reads it sees the same bytes I do.
- **Source preservation is byte-level** — when I edit, only what I edited changes. No reformat-on-save, no whitespace normalization, no "helpful" cleanup.
- **HTML and Markdown both render natively** — rich, properly styled, with the agent's intent preserved (CSS, SVG, inline diagrams, layout — not stripped down).
- **Light edits in place** are friction-free — small fixes, typos, restructuring. The heavy edits go back to the agent.
- **Roundtripping to the agent is one motion** — at minimum, a "copy this with my note for Claude" affordance that hands off to whatever Claude surface I'm running.
- **iCloud Drive is the sync substrate** — open from the same folder on Mac and (eventually) iOS without thinking about it.

## Shape Constraints

This is what Vellum is NOT:

- Not a writer's tool. It's a reader's tool that also lets you edit.
- Not a code editor.
- Not a browser. (It renders HTML; it doesn't navigate the web.)
- Not multiplayer.
- Not a CMS.
- Not a JS sandbox for arbitrary LLM HTML to execute. (Render in a sandboxed surface; do not auto-execute scripts unless explicitly invoked.)
- Not a compliance product.
- Not a startup.
- Not an "AI editor" with embedded model calls. The agent lives outside Vellum; Vellum hands off and ingests.
- Not a Notion replacement. No databases, no team workspaces.

## Stance Commitments

- **Local by default.** No cloud account requirement. iCloud Drive is the user-chosen sync substrate, not a Vellum service.
- **Open source by default.** Apache 2.0 from commit #1. Public repo.
- **Personal by default.** Single-user design choices win over multi-user generality.
- **Read-first.** 80/20 read-to-edit ratio. Optimize ingestion above all.
- **Format-faithful.** What the agent wrote is what gets saved. No silent rewrites.
- **Markdown and HTML are co-equal.** Markdown ships first; HTML follows as a P0 add when MD is solid. No HTML afterthought, no Markdown afterthought.
- **Mac-first, iOS-later.** Validate utility on Mac before forking to native Swift on iOS. Tauri stays the build target for now.
- **Roundtrip is the new edit.** The loop with the agent is the product motion. Vellum facilitates the handoff; the agent does the heavy lifting.

## Explicit Uncertainties

- Whether the "review-and-roundtrip" pattern holds up under daily use, or whether I just keep falling back to opening artifacts in Claude Desktop / Browser / VS Code.
- Which roundtrip mechanism is right (pasteboard handoff, URL-scheme handoff to Claude Desktop, filesystem queue, direct API). Pasteboard is the leading hypothesis; the rest are open.
- How rich HTML rendering needs to be — whether sandboxed iframe rendering is enough, or whether Vellum needs custom layout work for specific Claude-output patterns.
- Whether ProseMirror is the right rendered-view substrate for Markdown given the read-first stance, or whether a simpler renderer (markdown-it → HTML → DOM) is the lower-friction path.
- Whether light editing of HTML is something I'll actually want, or whether I'll always route HTML edits through the agent (in which case source view is enough and "edit in place" only applies to Markdown).
- Whether the iOS step ever happens, or whether daily use plateaus on Mac.
- Whether the open-source posture survives contact with what is now a more legible product surface than v1.0 described.

## What v2.0 supersedes from v1.0

v1.0 framed Vellum as a personal Markdown editor with embedded executable blocks (`vellum:live-query`), an MCP trust model, and "honest history" of block executions. That framing was solving an imagined workflow — referencing live external data from inside Markdown docs — that didn't match how I actually work.

v2.0 retains:
- Single-user-by-design
- Apache 2.0 open source
- Local by default
- Plain-file format
- Byte-level source preservation
- Rust core + Tauri shell (for now)
- Atomic save with corruption guard

v2.0 explicitly drops:
- `vellum:live-query` primitives
- MCP client / `rmcp` integration
- `~/.vellum/trust.toml` trust model
- Evidence State badges, runs.ndjson, frozen-vs-live diff
- The "blocks can run" differentiator
- Days 31-60 and Days 61-90 BACKLOG as previously scoped

These features may still be valuable to someone; they are not load-bearing for the actual job-to-be-done I'm pointed at.
