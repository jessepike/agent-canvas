---
project: vellum
status: locked
version: 1.0
updated: 2026-05-10
---

# Vellum — Intent

## Outcome

A desktop Markdown editor where blocks can run, the file stays plain, and history is honest. Built for one person — me — to use every day. Open source from day one. If anyone else likes it, that's a bonus.

## Why It Matters

The way I work is: Markdown files in folders, opened in an editor, with data and references that have to stay fresh. The tools today fail at one of three things:

- They corrupt my files. (Most rich editors.)
- They can't run anything. (Most plain editors.)
- They lie about what changed. (Everything that calls itself "live.")

Vellum exists because none of them get all three right at once. The constraint of "build it only for me" cuts every product decision that doesn't serve daily use.

## Problem I'm Pointed At

A daily-driver Markdown environment where:

- The file is the durable artifact, openable in any editor in ten years.
- Blocks can execute (call MCP tools, render their results inline) without trapping the content in a proprietary format.
- The history of every block's execution is recorded honestly — what ran, what came back, when, with what hash.
- Source preservation is byte-level, not best-effort. Files do not silently mutate.

## Shape Constraints

This is what Vellum is NOT:

- Not a Notion replacement. No databases, no team workspaces.
- Not a code editor.
- Not a browser.
- Not multiplayer.
- Not a CMS.
- Not a JS sandbox.
- Not a compliance product.
- Not a startup.

## Stance Commitments

- **Local by default.** No cloud account requirement.
- **Open source by default.** Apache 2.0 from commit #1. Public repo, no private prelude.
- **Personal by default.** Single-user design choices win over multi-user generality.
- **The Rust core is where durability lives.** UI is a thin shell.
- **Source preservation is the existential property.** Format-preservation corpus is the v1 build gate.

## Explicit Uncertainties

- Whether the ProseMirror ↔ Rust block-source patch model holds up under heavy edit-and-save cycles in real use.
- Whether `pulldown-cmark`'s `OffsetIter` grounds the partition contract for all hard Markdown cases (HTML blocks, link reference definitions, lazy continuation), or whether we fall back to `markdown-rs`.
- Whether the trust toml model scales beyond ~50 MCP servers (not a near-term concern).
- Whether the single-user constraint survives contact with reality once the repo is public.

These are named risks the spec accepts going into scaffolding.
