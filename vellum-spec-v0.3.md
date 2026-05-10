# Vellum — Design Spec v0.3

**Status:** Locked for scaffolding
**Owners:** Jess + Claude
**Working name:** Vellum
**Last updated:** 2026-05-09
**License intent:** Apache 2.0
**Reviews applied:** GPT-5.5 round 1, GPT-5.5 round 2, Claude CPO cycle 1–3 (internal), Codex external (implementation lens), Claude -p external rounds 1–2 (architectural lens)

---

## Headline

> **Live Markdown. Plain files. Honest history.**

A desktop Markdown editor where blocks can run, the file stays plain, and history is honest.

## The constraint

Vellum is built first and only for the person building it to use every day. There is no buyer, no beachhead, no design-partner program, no enterprise procurement. The user is the author. Open source from day one. If anyone else likes it, that's a bonus, not a strategy.

This constraint clarifies the design more than any review round did. It cuts the trust-policy-engine UX, the evidence-bundle export, the auditable-compliance language, the GTM-shaped product decisions, and the buyer-comforting fluff. What remains is a tool with engineering integrity that works for one person. That is enough.

---

## Delta from v0.2

- **Headline reframed.** *"Live evidence documents that can be frozen, regenerated, and defended"* → **"Live Markdown. Plain files. Honest history."** Compliance language gone. Provenance reframed as personal memory, not audit trail.
- **Constraint declared.** Single-user tool. No beachhead, no design partners, no GTM. Open source Apache 2.0 from day one.
- **Trust UX simplified.** Permission profile matrix removed from UX. One personal config: which MCP tools and which MCP servers (with binding for anti-spoofing) are trusted. Internal capability model retained for engine correctness.
- **Source-preserving Markdown save** elevated to a dedicated architectural section. Block-level source preservation is the existential property of v1.
- **iOS pulled forward to v1.5.** Read + light-edit SwiftUI viewer reading a vault from iCloud Drive. ~2-week build, post-v1.
- **Vault doctor cut from v1.** Inline validation at authoring time replaces it (see §Primitive validation). Doctor reconsidered post-v1 if a clear pattern of validation gaps emerges.
- **v1 primitive set narrowed to one.** Only `vellum:live-query` ships in v1. Agent, transform, and include deferred to v1.5+.
- **Cut from v1:** Tantivy search UI, dataflow-grant UI, evidence-bundle export, renderer manifest spec, show-as-of historical render, structural table diff, plugin model artifacts.
- **Kept and sharpened:** Tauri + Rust core; ProseMirror authoritative; source-preserving save; versioned primitive schema; sidecar cache; freeze command; frozen-vs-refreshed diff; Evidence State per primitive; conflict-safe save.

---

## Vision

The way I work is: Markdown files in folders, opened in an editor, with data and references that have to stay fresh. The tools today fail at one of three things: they corrupt my files (most rich editors), they can't run anything (most plain editors), or they lie about what changed (everything that calls itself "live").

Vellum is the editor I want to use. Markdown stays Markdown. Blocks can call MCP tools and render their results inline. When a result changes between today and last week, I can see what changed. When I freeze a doc, it stays frozen. When I open the file in any other editor in ten years, it reads cleanly.

That's it.

## What it is not

- Not a Notion replacement. No databases, no team workspaces.
- Not a code editor.
- Not a browser.
- Not multiplayer.
- Not a CMS.
- Not a JS sandbox.
- Not a compliance product.
- Not a startup.

---

## Core principles

1. **The Markdown file is the durable source artifact.** Plain text, machine-portable, openable anywhere.
2. **Vellum does not silently rewrite my files.** Source-preserving block-level save. Whitespace, comments, table alignment, fence lengths, frontmatter style — all preserved unless the user explicitly edits them.
3. **Lossless duality.** Readable like a document. Inspectable like source. One keystroke between them.
4. **Blocks execute with honest history.** Every primitive run is recorded. Every result has a hash. The file shows what state it's in.
5. **The Rust core is where durability lives.** Filesystem, MCP, cache, indexer, primitive runtime, hashing. The UI is a thin shell.
6. **Local by default. Open source by default. Personal by default.**

---

## Architecture

```
+--------------------------------------------------+
|  Tauri Shell (macOS / Windows / Linux)           |
|                                                  |
|  +--------------------------+                    |
|  | WebView (TS + React)     |                    |
|  |   ProseMirror            |  authoritative     |
|  |     (live edit model)    |  for live editing  |
|  |   CodeMirror 6           |  source view       |
|  |   Primitive renderers    |  pure components   |
|  +-------------+------------+                    |
|                |                                 |
|       IPC: typed (serde <-> zod), block-grain    |
|                |                                 |
|  +-------------v------------+                    |
|  | Rust Core                |                    |
|  |   File system layer      |  source-preserving |
|  |   MD block parser        |                    |
|  |   MCP client             |                    |
|  |   Primitive runtime      |                    |
|  |   Trust manager (light)  |                    |
|  |   Cache + run log        |                    |
|  |   Hash engine            |                    |
|  +--------------------------+                    |
+--------------------------------------------------+
         |                       |
   [user filesystem]       [MCP servers]
```

**ProseMirror owns live editing.** Rust does not maintain a parallel editing tree. The Rust side parses Markdown into blocks for primitive identification and source-preservation tracking, but it does not compete with ProseMirror for editing authority. IPC is block-grain.

### Block identity (the load-bearing detail)

Source preservation requires answering "is this block the same one I parsed last time?" reliably, even after structural edits to neighbors. Two identity systems work in tandem:

- **Primitives carry stable `id` fields inside their fenced YAML payload.** Identity travels with the block and survives any edit that doesn't change the `id`. This is the on-disk identity and is canonical for primitives.
- **Non-primitive blocks (paragraphs, headings, lists, HTML, frontmatter, tables, blockquotes) get in-memory stable IDs assigned at parse time.** Each ProseMirror node corresponding to a parsed block carries a UUID via PM document metadata (NOT written to disk). On save, Vellum walks the PM tree and resolves each block per the branch rule below ("known unchanged" / "known changed" / "new").

The non-primitive ID map persists across sessions in `<vault-root>/.vellum-cache/<docpath>/identity.json` for warm-start coherence.

**Sidecar lifecycle, including rename/move:** on open, if the expected sidecar is absent at `<docpath>/identity.json`, Vellum scans `<vault-root>/.vellum-cache/` for any `identity.json` whose recorded source-hash matches the file being opened, and migrates it to the new path. If no match is found, the doc enters cold state. Rename or move inside the vault is therefore the common-case path that resolves automatically; only outright sidecar deletion or a vault-cache wipe drops to cold state.

**Cold-state behavior (sidecar absent, no migration found):** raw bytes for every block are still in memory at parse time — the parser produces (block, byte_range, parsed_kind) tuples regardless of any identity map. Fresh UUIDs are assigned at parse for every non-primitive block. **A no-edit open→save in cold state emits raw bytes per block and is byte-identical to the original.** Identity matters only for tracking blocks across edit sessions — not for byte-preservation on a no-op round trip. After an edit-and-save in cold state, only the edited blocks re-serialize through the Markdown serializer; untouched blocks emit their preserved raw bytes.

**The format-preservation corpus runs at both gates:**
- Steady-state gate (sidecar present, no edits, save) → byte-identical.
- Cold-state gate (sidecar absent, no edits, save) → byte-identical.

Block identity does not depend on tree position, line number, or content alone.

**Branch resolution rule on save** (per non-primitive block):
- Known ID, content + structure unchanged from last parse → emit preserved raw bytes.
- Known ID, content or structure has changed since last parse → re-serialize via Markdown serializer.
- New ID (block created during this session) → serialize fresh.

For primitives, the `id` field on disk is canonical; the same three-branch rule applies, with the additional pre-save case below.

**Primitive `id` lifecycle.** A freshly-authored primitive may legally lack an `id` in transit. On first save, if `id` is absent, Vellum auto-injects a new UUID-derived `id` alongside `created_at`. Inline validation surfaces a non-blocking "primitive has no id yet — will be assigned on save" hint while authoring. User-authored ids are preserved verbatim and never overwritten. (See §Primitive schema for the authoritative field-status list.)

### Block patch contract (the IPC shape between PM and Rust)

This is the contract that flows across the IPC boundary. It is the surface where corruption bugs concentrate; it is written here so scaffolding does not invent it ad-hoc.

```rust
// Rust side (mirrored to TS via the IPC type generator)
pub struct BlockPatch {
    pub block_id: BlockId,                // primitive: YAML id; non-primitive: UUID
    pub parsed_kind: BlockKind,           // Heading | Paragraph | List | BlockQuote | ...
    pub original_byte_range: Option<Range<usize>>,  // None for newly-created blocks
    pub edit: BlockEdit,
    pub dirty: bool,                      // true if PM content differs from last parse
    pub error: Option<BlockError>,        // populated when validation rejects this patch
}

pub enum BlockEdit {
    PreservedBytes,                       // emit original_byte_range bytes verbatim (cheapest)
    EditedBytes(String),                  // user edited in source view; emit these bytes verbatim
    SerializeFromTree,                    // user edited in rendered view; serialize from PM
}
```

**Edit-source preference rule (load-bearing):** when the edit originated in source view OR inside a primitive's YAML body, `BlockEdit::EditedBytes(s)` is preferred over `SerializeFromTree`. Editing one inline token in rendered view should not cause the whole block's formatting to be rewritten. The serializer is the fallback path, not the default.

**Merge / split / delete operations** produce sequences of `BlockPatch`:
- **Split** (one block becomes two): emit two patches with the same `parsed_kind`, the upper carrying `original_byte_range` truncated and `EditedBytes` for the kept-and-modified prefix; the lower carrying `original_byte_range: None` and `EditedBytes` for the new content. Block-separating whitespace is owned by the upper.
- **Merge** (two blocks become one): emit one patch carrying `original_byte_range: None`, `EditedBytes(combined)`, and the surviving `parsed_kind`. The lower's range disappears from the partition.
- **Delete**: omit from the patch sequence; the partition closes over the gap.

**Save flow.** PM produces an ordered `Vec<BlockPatch>` representing the current document. Rust verifies the partition invariant (concatenated ranges + edited bytes still produce a valid byte sequence), runs each patch's resolution rule, concatenates the result, then runs the atomic write path (§Rust core). A patch sequence that violates the partition invariant aborts the save and surfaces an error rather than emitting a corrupted file.

**Error model.** `BlockError` is one of: `Overlapping(other_id)`, `GapBefore(prev_id)`, `InvalidYaml(reason)` (primitives only), `DuplicateId(other_id)` (primitives only), `MissingRequiredField(field)` (primitives only). All errors are surfaced inline at authoring time per §Primitive validation; save is blocked until all errors are cleared.

---

## Source-preserving Markdown save (the existential property)

If Vellum corrupts your files, nothing else matters. This section is the load-bearing wall.

### Principle

Vellum must not pretty-print, normalize, or rewrite user Markdown by default. The on-disk file should be a stable artifact across editing sessions, regardless of how many times it has been opened, edited, and saved.

### Architecture

- **Block-level source preservation.** On open, Rust parses the file into a sequence of blocks. Each block retains its **raw source byte range** in addition to its parsed structure.
- **Edits produce block-level patches.** A user edit to a paragraph mutates only that block's source. Untouched blocks serialize back as their preserved raw bytes — not re-rendered from a parsed model.
- **Fenced primitive blocks are special.** When the user edits the *body* of a primitive via UI affordances (rerun, change cache, freeze), Vellum mutates only the YAML lines that changed. Auto-managed fields (`content_hash`, `last_run_at`) update in place.
- **Whitespace, blank lines, and trailing characters are preserved at the block boundary.** No "normalize on save."
- **No re-flow of paragraphs.** A 200-character line stays 200 characters. A line-broken paragraph stays line-broken.

### Block boundary reconstruction (parser approach)

`pulldown-cmark` exposes an `OffsetIter` yielding `Range<usize>` per event, but events are tokens, not blocks. Vellum builds a thin layer on top.

**Partition contract (the rule the algorithm must satisfy):** the parser produces an **ordered, non-overlapping byte-span partition of the entire file**. Concatenating the byte ranges of every emitted top-level block in order reproduces the file byte-for-byte. There are no gaps, no overlaps, and no spans nested inside other top-level spans.

- **Top-level blocks (each owns one preserved-bytes span):** Heading, Paragraph, List (the entire list, not its items), BlockQuote, CodeBlock, HtmlBlock, Table, FootnoteDefinition, ThematicBreak, plus a single opaque Frontmatter block at the top of the file when present. Link reference definitions are top-level blocks and **must** round-trip without being absorbed into the paragraphs that consume them — this requires the parser to use `pulldown-cmark`'s reference-definition pass (`Options::ENABLE_FOOTNOTES` is unrelated; see `pulldown-cmark` docs for the actual flag) rather than relying on event frames.
- **Inner blocks (Item inside List, Paragraph inside BlockQuote, etc.) are tracked for parsing fidelity and ProseMirror schema fidelity but DO NOT produce independent preserved-bytes spans.** The outermost block-frame owns the raw byte range; inner frames inform the PM tree only.
- **Algorithm:** event-stream walker that opens a "block frame" only on top-level block-start events. The block's byte range is `(first_event.range.start, last_event.range.end)` extended forward through any immediately-following block-separating whitespace and stopping at either the start of the next top-level block or end-of-file. Inter-block whitespace is owned by whichever block comes before it.
- **Hard cases (corpus must cover):** HTML blocks (HTML5 block-level vs CommonMark Type-1–7 distinctions), link reference definitions (must round-trip without being inlined into paragraphs that consume them), footnote definitions (`[^id]:` blocks), lazy continuation lines inside lists/blockquotes, fenced code blocks with non-standard fence lengths and info strings, indented code blocks adjacent to lists, setext vs ATX heading style, BOM, CRLF, trailing newline-vs-no-newline at EOF.
- **Frontmatter** is treated as a single opaque top-level block (always the first block when present) with its raw bytes preserved verbatim. YAML, TOML, and `+++`-delimited variants are all recognized but never re-serialized by Vellum.
- **Fallback gate:** if `pulldown-cmark` cannot ground the corpus byte-identical for an irreducible class of files, the parser swaps to `markdown-rs` (which has a CommonMark AST with positions) before considering `tree-sitter-markdown`. This is a documented fallback, not a default.

The partition contract is checked by an invariant test in CI: for every corpus file, `Σ block.byte_range == 0..file.len()` and ∀ adjacent blocks `(a, b)`: `a.range.end == b.range.start`. The custom layer ships in `crates/vellum-core/src/parse/` and is the first piece built. Editor work does not begin until the corpus passes against this layer at the steady-state gate.

### Format preservation test corpus

Built before any editor functionality ships. A directory of `.md` files representing every edge case I care about — frontmatter variants, table alignment styles, code fence lengths, HTML comments, footnotes, link reference definitions, mixed list styles, hard line breaks, trailing whitespace, BOM, CRLF line endings.

The contract: open every file in the corpus, edit nothing, save. The file on disk must be **byte-identical** to the original. This is the v1 build gate. Editor work does not begin until this passes.

### When Vellum will modify content it didn't author

Only with explicit user action and a visible diff:

- "Format document" command (off by default, never on save).
- Edits that touch a block's source.
- Auto-managed primitive fields (`content_hash`, `last_run_at`) — only inside `vellum:*` blocks the user already owns.
- Save-time conflict resolution (after the user reviews a diff).

### When external changes happen

File watcher detects external modification. UI shows a three-pane diff before allowing overwrite. No silent auto-merge.

- **In-memory** — current ProseMirror state, what the user has been editing.
- **On-disk** — the bytes that just landed on the filesystem from the external writer.
- **Base** — the bytes Vellum read on open (or last successfully saved). This is the common ancestor for the visual three-way merge.

If the doc was opened freshly and never edited, the in-memory and base panes match; the diff is effectively two-pane.

---

## The Rust core

Owns:

- **File system layer.** Atomic writes via **same-directory tmpfile** (`<doc>.vellum-tmp-<pid>-<short_uuid>` adjacent to target — pid alone is insufficient when two Vellum processes touch the same vault), then `rename(2)` — never a system tmpdir, to keep the rename inside one volume and avoid silent copy-degradation on network or external-drive vaults. **Save precondition (the corruption guard, never optional):** immediately before `rename(2)`, the save path stat+hashes the current target file and compares against the in-memory base hash (the bytes Vellum read on open or last successfully saved). If the on-disk hash differs, the watcher missed an event and `rename` would overwrite a newer file — abort to the three-way conflict flow (§When external changes happen) and do not commit. Tmpfile cleanup: on vault open, Vellum reaps stale `*.vellum-tmp-*` files whose pid is not a live Vellum process. `notify`-backed watching, external-change diff prompt, conflict-marker detection on open (Git-style `<<<<<<<` / `=======` / `>>>>>>>`), lock-free concurrent reads.
- **Block parser.** `pulldown-cmark` for parsing into block structures with preserved source byte ranges. Custom serializer that emits raw source for unchanged blocks.
- **MCP client.** Built on `rmcp`. Server config, capability discovery, auth lifecycle.
- **Primitive runtime.** Schema validation, capability checks against personal trust config, execution lifecycle, cache management, streaming results.
- **Trust manager (light).** Personal config: which MCP tools the user trusts, which MCP servers the user trusts. Stored in `~/.vellum/trust.toml`. No per-doc grant matrix.
- **Cache and run log.** Vault-rooted `<vault-root>/.vellum-cache/<docpath>/` with results, append-only `runs.ndjson`, and `identity.json` (non-primitive block ID map).
- **Hash engine.** `blake3` for content, args, results. Used for change detection, not certification.

Primary crates (load-bearing pins; floor versions, will track latest stable): `tauri 2.x`, `tokio 1.x`, `serde 1.x`, `notify 6.x`, `rmcp` (latest at scaffold time — young crate, expect churn), `pulldown-cmark 0.13+` (for `OffsetIter`), `blake3 1.x`, `tracing 0.1`, `thiserror 1.x`, `time 0.3`, `toml 0.8+`.

## The UI shell

- **Framework:** TypeScript + React + Vite.
- **Editor (authoritative):** ProseMirror direct, custom schema with primitive node types. No Tiptap.
- **Source view:** CodeMirror 6 with the Markdown grammar.
- **Sync:** Bidirectional, but **not per-keystroke**. Source view reflects the current block-source state on toggle and on save. Edits in source view apply to the parsed block model on toggle back. This avoids two competing live models.
- **Layout:** Split-view default. Rendered-only and source-only modes available; preference persists per doc.
- **Themes:** Vellum Light, Vellum Dark, optional `~/.vellum/theme.css`.

---

## Primitive schema (v1: `vellum:live-query` only)

````markdown
```vellum:live-query
version: 1
id: open-issues
created_at: 2026-05-09T14:22:00Z
tool: github.list_issues
args:
  repo: jessepike/vellum
  state: open
cache: 60s
result_policy: pinned
content_hash: blake3:9f2a...
last_run_at: 2026-05-09T14:31:12Z
render: table
```
````

**Required fields (user-authored):** `version`, `tool`, `args`.
**Auto-injected on first save (if absent):** `id` (UUID-derived), `created_at`. User-authored values for these fields are preserved verbatim; auto-injection only fills absent values.
**Optional with defaults:** `result_policy` (default `pinned`), `render` (default `json`), `cache` (default: cache forever — manual refresh only). Unknown `render` values are an inline-validation warning.
**Auto-managed (updated on each run):** `content_hash`, `last_run_at`.

The persisted `content_hash` is a cached echo of the canonical recipe hash; the live recipe hash (computed at load and on every recipe edit) is authoritative at runtime, especially for the `vellum:result.recipe_hash` comparison that drives `Frozen` vs `Changed-since-frozen` Evidence State. The persisted field is only updated on save and successful run.

**`content_hash` is computed over the canonical recipe** — `blake3(version || tool || canonical_yaml(args))` — and explicitly excludes auto-managed fields and any `cache` window setting. A recipe edit changes the hash; a cache-window edit does not.

**`cache` field accepts Go-style durations** (`60s`, `5m`, `1h`, `24h`). Plain integer is rejected. `0s` means "always re-run when visible"; absence of the field means "use cache forever (manual refresh only)."

**`result_policy` values:**

- `transient` — result is not persisted across sessions.
- `pinned` — result persists in `<vault-root>/.vellum-cache/<docpath>/<id>.json`. **Default.**
- `inline_snapshot` — result writes to an adjacent `vellum:result` block (with `for_id` matching the primitive's `id`) in the .md file. Explicit archival mode, not default. See §`vellum:result` below for schema.

**Defaults are sidecar-first.** Plain Markdown stays plain during normal work. Inline snapshots are an explicit user action ("Freeze for archive"), not automatic.

### `vellum:result` — the inline snapshot block

When `result_policy: inline_snapshot` is set OR the user runs the manual freeze command, an adjacent fenced block is written immediately after the primitive:

````markdown
```vellum:result
for_id: open-issues
recipe_hash: blake3:9f2a...
result_hash: blake3:b701...
captured_at: 2026-05-09T14:31:12Z
render: table
data: |
  [{"number": 17, "title": "..."}, ...]
```
````

**Required fields:** `for_id` (must match a primitive `id` in the same doc), `recipe_hash` (the canonical recipe hash at the moment of capture — used for "Changed-since-frozen" detection), `result_hash`, `captured_at`, `data`.

**`data` is a YAML-quoted string, canonical-JSON encoded** regardless of `render`:
- The Rust core captures the tool result, encodes it as canonical JSON (deterministic key ordering, no whitespace), and stores that string in `data`.
- Renderers consume the parsed JSON. `json` displays it pretty-printed; `markdown` parses `data` as a JSON-encoded string and renders the inner text as Markdown; `table` / `list` / `card` parse it as JSON arrays/objects; `metric` parses it as a JSON scalar object with at least a `value` field.
- This unifies hashing and storage; renderer-specific shape lives in interpretation, not encoding.

**`result_hash = blake3(canonical_json(tool_response))`**, computed in the Rust core before any renderer transform. This makes the run log and the frozen-vs-refreshed diff reproducible.

A `vellum:result` block whose `for_id` does not resolve to a primitive in the same doc surfaces as an inline-validation warning ("orphan snapshot") and offers a "remove" action. Two `vellum:result` blocks for the same `for_id` is a validation error and the doc switches to source-only safe mode until resolved. The recipe is canonical; the result is its echo.

**Resolution is doc-scoped, not adjacency-scoped.** The freeze command always writes the `vellum:result` block immediately after its primitive, but if the user moves it elsewhere in the doc, the `for_id` linkage still resolves and the Frozen state still functions. Adjacency is the convention, not the rule.

**Two distinct lifecycles for the `vellum:result` block:**

- **Manual freeze** (user runs the freeze command on a primitive whose `result_policy` is `transient` or `pinned`): one-shot. The `vellum:result` block is written once and is **never auto-overwritten**. If the recipe is later edited (live recipe hash ≠ snapshot's `recipe_hash`), the Evidence State surfaces as `Changed-since-frozen` until the user explicitly re-freezes or removes the snapshot. This is the freeze contract.
- **`result_policy: inline_snapshot`** (declared on the recipe): the block is **overwritten on every successful run**. The recipe is the source of truth; the inline result is its current echo. `Changed-since-frozen` does not apply to this policy because the block re-writes on each run; if the recipe is edited, the next run overwrites the block.

**Manual freeze is unavailable when `result_policy: inline_snapshot`.** The policy itself is the freeze surface; allowing both would produce two `vellum:result` blocks for the same `for_id`, which the validation rule treats as a hard error. The UI hides the manual-freeze affordance for `inline_snapshot` primitives.

The two policies serve different intents (archival snapshot vs always-current echo) and the spec keeps both.

**`render` values for v1:** `table`, `list`, `card`, `json`, `markdown`, `metric`. Built-in renderers only. No third-party rendering.

### Primitive validation

Inline at authoring time, not deferred to a doctor:

- duplicate `id` rejected at paste/save (paste UI offers "regenerate id" affordance)
- unknown tool (not in trust config and not advertised by any connected MCP server) warns immediately on edit
- invalid YAML highlighted in source view
- last-run result older than the primitive's `cache` window is flagged with a refresh affordance (this is the steady-state "stale result" signal — distinct from `content_hash` mismatch, which means the recipe itself was edited and the cached result is for a different recipe)
- orphan `vellum:result` blocks (no matching `for_id`) surface as inline warnings
- conflict markers on open switch the doc to source-only safe mode

---

## Evidence State (per primitive)

The user-facing model. Every rendered primitive shows one state:

- **Live** — last run is fresh, within cache window. Primitives with no `cache` field remain Live until the recipe changes or the user manually refreshes; primitives with `cache: 0s` are Live only during the visible run frame and re-run on every visibility transition.
- **Cached** — last run is stale by cache policy but not yet refreshed.
- **Frozen** — `vellum:result` block exists (from a manual freeze) and its `recipe_hash` matches the primitive's current `content_hash`. Document is showing recorded state.
- **Changed-since-frozen** — `vellum:result` block exists (from a manual freeze) but its `recipe_hash` no longer matches the primitive's current `content_hash`. The frozen result no longer represents the current recipe. (Does not apply to `result_policy: inline_snapshot`, which auto-overwrites.)
- **Broken** — last run failed. Tool unreachable, schema invalid, args rejected.
- **Untrusted** — primitive references a tool not in the user's trust config.

A small badge on each rendered primitive shows the state. Click expands a panel with last-run timestamp, duration, result hash, "show recipe" toggle, and "diff vs last frozen result" button.

This is what the user reads. Hashes are infrastructure. State is the product.

---

## Trust (personal config)

A single file: `~/.vellum/trust.toml`.

```toml
[tools]
"github.list_issues" = "trusted"
"github.create_issue" = "ask"
"slack.list_messages" = "trusted"
"slack.send_message" = "ask"

[servers]
"github" = { state = "trusted", bind = "command:gh-mcp" }
"slack" = { state = "trusted", bind = "url:https://slack.example/mcp" }
"local-mcp.*" = { state = "trusted", bind = "command:local-mcp-*" }

[defaults]
unknown_tool = "ask"
unknown_server = "ask"
```

Three states per tool/server: `trusted` (auto-run), `ask` (confirm each invocation, with rationale: tool name + args summary + persistent-grant toggle — this is what makes `ask` not click-through), `block` (refuse).

**Server identity binding (anti-spoofing).** A server entry is a record `{ state, bind }`. `bind` is one of `command:<exec-spec>` (matches the configured MCP server's invocation command), `url:<scheme://host[/path]>` (matches the configured server's transport endpoint), or `*` (any binding — explicitly opts out of spoof protection, only legal for the `[defaults]` table). Trust evaluation requires BOTH the advertised server-identity-name AND the `bind` to match against the configured server entry; a malicious server claiming `github` but invoked via a different command resolves as an unknown server and falls to `unknown_server`. Trust does not flow from handshake-name alone.

**Trust evaluation order (deterministic):**
1. **Server gate first.** Resolve the server identity (advertised name + binding). Lookup in `[servers]`. If the entry's `state` is `block`, the call is refused; tool gate is not evaluated.
2. **Tool gate second.** Lookup the tool key in `[tools]`. If found, the tool's state is final.
3. **Defaults.** If a server is unknown (no `[servers]` entry matches name+bind), `unknown_server` decides. If known but the tool is unknown, `unknown_tool` decides.

**Tool-level `trusted` cannot override server-level `block`.** A `block` at server level short-circuits. An explicit `trusted` tool with the server set to `ask` results in `trusted` (tool gate is more specific than the server's "ask"). This is the rule that makes the precedence honest: server-block is a kill switch, server-ask is a fallback.

**Default `unknown_server = "ask"`** (changed from `"block"`): a brand-new server that handshakes and binds correctly produces a first-call prompt, which makes the "trust prompts on first call to a new tool" milestone actually reachable. If a user wants no-unknown-servers-ever, they set `unknown_server = "block"` deliberately.

**Pattern syntax (server keys, tool keys):** keys are TOML strings. A trailing `.*` matches **any number of trailing namespace components** — `local-mcp.*` matches `local-mcp.foo` and `local-mcp.foo.bar` and `local-mcp.a.b.c`. Single `*` is the only wildcard recognized in v1; full glob and regex are deferred. Most-specific match wins (literal > pattern). Tiebreaker between equally-specific patterns: order of declaration in the TOML file (first match wins).

That's it. No per-doc grant matrix. No dry-run plan UI for untrusted strangers. No dataflow grant separate from tool grant. The internal engine still tracks capabilities for correctness, but the user-facing surface is the toml above and a settings panel that edits it.

A primitive blocked at the server level shows Evidence State `Untrusted` with a "Trust this server…" button that opens a server-trust prompt; a primitive blocked only at the tool level shows the same state with a "Trust this tool" button. Either click updates the toml.

For the rare case where a vault is shared (open source, friends, future), a portable `.vellum/trust.manifest.toml` declares what tools and server-bindings the vault expects to call. Opening on a new machine shows what's missing in the user's personal trust config.

---

## File and on-disk semantics

- **Format:** GFM Markdown + YAML frontmatter + `vellum:*` fenced blocks.
- **Extension:** `.md`. No new extension.
- **Vault config:** `<vault>/.vellum/config.toml` for vault-shared settings.
- **Vault trust manifest:** `<vault>/.vellum/trust.manifest.toml` (optional, for shared vaults).
- **Personal config:** `~/.vellum/config.toml` and `~/.vellum/trust.toml`.
- **Cache:** **`<vault-root>/.vellum-cache/<docpath>/`** — vault-rooted, with the doc's path-from-vault-root as subfolder hierarchy. (Not sibling-to-doc; not collapsed by basename.) Holds `runs.ndjson`, pinned results, and `identity.json` (the non-primitive block ID map).
- **`.gitignore` template generated alongside.** Cache is local-only. Personal trust is local-only. Vault config is committable.

### State decomposition

- **In file:** prose, primitive recipes, primitive ids, optional inline snapshots.
- **Vault-shared (committed):** `config.toml`, `trust.manifest.toml`.
- **Personal/machine-local:** `~/.vellum/`, OS keychain (credentials via MCP server config).
- **Generated (gitignored):** `.vellum-cache/`.

### Portability contract

- Any Markdown editor reads and edits the file.
- Vellum re-executes when compatible MCP tools are configured locally.
- Reproducing past state requires inline snapshots or pinned cache.
- Trust and credentials are intentionally not portable.

---

## Performance envelopes

| Doc class | Size | Behavior |
|---|---|---|
| Small | <250KB | Full sync, all features |
| Normal | 250KB–2MB | Full sync, all features |
| Large | 2MB–10MB | Lazy parse, virtualized rendering |
| Huge | 10MB+ | Source-first or sectional rendered, degraded |

| Metric | v1 target |
|---|---|
| Cold start | < 1.5s |
| Open 1MB doc to first editable view | < 300ms |
| Open 10MB doc | < 2s |
| Source ↔ rendered toggle (normal class) | < 100ms |
| Primitive cached render (visible) | < 100ms |
| Idle memory, 10 normal docs open | < 500MB RSS |

Exit criterion: open and edit a 2MB Markdown doc with 50 primitives, smooth scroll, stable primitive state across edits and saves. **Format preservation test corpus passes byte-identical.**

---

## 30 / 60 / 90 milestones (v1, locked)

**Days 1–30 — Source-preserving editor core**

Two gates within this milestone. The parser and corpus are the load-bearing wall and ship first.

*Gate 30A — Parser + corpus (target: ~day 15–18)*

- Tauri scaffold; **IPC contract types: `ts-rs` for TypeScript types + handwritten Zod schemas at the IPC boundary** (`ts-rs` does not generate Zod runtime validators by itself; the boundary requires both static types and runtime validation). Type-tests in CI verify the generated TS shape matches the Zod schema.
- Format preservation test corpus assembled (50+ files; covers all hard cases listed in §Block boundary reconstruction).
- Block parser with raw source byte-range preservation on top of `pulldown-cmark`.
- Atomic writes (same-volume tmpfile → `rename(2)`), file watching, conflict-marker detection.
- Sidecar identity map (`<vault-root>/.vellum-cache/<docpath>/identity.json`) with cold-state fallback.

**Exit criterion 30A:** Format preservation corpus passes byte-identical at BOTH the steady-state (identity sidecar present) and cold-state (identity sidecar absent) gates. CI runs the corpus on every commit.

*Gate 30B — Editor end-to-end (target: day 30)*

- ProseMirror custom schema with primitive node types and PM-decorated stable block IDs.
- CodeMirror 6 source view; toggle-time sync.
- External-change diff prompt UI on file watcher event.

**Exit criterion 30B:** Open and edit a 2MB doc with mixed primitive and prose blocks, save, reopen — file is unchanged except where the user typed; format preservation corpus still passes after the edit round-trip.

**Days 31–60 — `vellum:live-query` end-to-end**

- Versioned primitive schema parser/validator.
- Inline validation: duplicate id, unknown tool, invalid YAML.
- MCP client integration with capability discovery via `rmcp`.
- **First MCP server: GitHub MCP server (read-only tool subset).** Auth surface in scope: API token loaded from MCP server config, persisted in OS keychain. OAuth flows, device codes, and refresh-token rotation are explicitly **deferred to v1.5**. Second server is opportunistic, not a milestone gate.
- `~/.vellum/trust.toml` config + settings panel.
- Trust prompts on first call to a new tool (with rationale: tool name, args summary, persistent-grant toggle).
- Sidecar cache; `transient` and `pinned` result_policy.
- Built-in renderers: `table`, `list`, `card`, `json`, `markdown`, `metric`.

**Exit criterion:** Author a doc with three live-query primitives against the GitHub MCP server, open, run, render, edit, save, reopen, refresh. No file corruption. Format preservation corpus still passes.

**Days 61–90 — Honest history + ship**

- Append-only `runs.ndjson` with content/args/result hashes.
- Evidence State badges per primitive in rendered view.
- "Show recipe" toggle on every primitive.
- Manual freeze command — writes adjacent `vellum:result` block (manual-freeze lifecycle, never auto-overwritten).
- Frozen-vs-refreshed diff (text and JSON).
- Conflict-safe save (three-pane diff on external change).
- Settings panel: trust config, theme, cache mgmt.
- **v1.0 tagged release** on the already-public repo: binaries for macOS (Intel + ARM), Windows, Linux. **Signing posture for v1.0:** macOS = ad-hoc / developer-signed (NOT notarized — Apple notarization requires a paid Apple Developer Program enrollment and a separate notarization workflow that's a multi-day operational track on its own); Windows = unsigned (code-signing certificate is a separate purchase + workflow); Linux = unsigned binaries + checksum file. Notarization and code-signing are explicitly **deferred to v1.1**. README documents that macOS users will see an unidentified-developer warning on first launch and that the workaround is documented. Complete README, contributing guide, build instructions. (Repo has been public since commit 1 — see §Open source + project setup. Day 90 ships the *release tag*, not the *repo*.)

**Exit criterion:** I am using Vellum daily as my primary Markdown editor. v1.0 is tagged.

---

## v1.5 (post-90-day, in priority order)

1. **iOS reader.** SwiftUI app, opens vault from iCloud Drive, read + light edit (no primitive execution). ~2-week build for one person.
2. **`vellum:agent` primitive.** Inline LLM calls with streaming. Reuses trust + Evidence State machinery.
3. **`vellum:transform` primitive.** Typed declarative ops only — filter, map, sort, group, summarize, extract, template. Structured YAML, no expression DSL. JS lambdas explicitly deferred behind a separate trust tier.
4. **`vellum:include` primitive.** Embed another doc. Recursion-guarded.
5. **Tantivy global search** across the vault.
6. **Evidence bundle export.** Zipped `.vellum-evidence/` package with manifest, snapshots, hashes, frozen rendered HTML/PDF.
7. **`chart` renderer.** Chart.js inside the renderer contract.

---

## Anti-patterns we will not adopt

- AI sidebar. Primitives are inline.
- Cloud account requirement.
- Custom binary format.
- Real-time collaboration in v1.
- Plugin marketplace.
- Telemetry by default. Opt-in only, and probably never.
- Arbitrary JS in Markdown documents.
- Click-through security UX.
- Custom Rust doc tree as a parallel live editing model.
- Heroic universal performance budgets.
- Compliance/audit-grade language until the system earns it.
- "Pretty print on save."

---

## Open source + project setup

- **License:** Apache 2.0. Permissive, patent grant, contribution-friendly.
- **Repository:** Public GitHub from the first commit. No private prelude.
- **Branch model:** Trunk-based. PRs from forks. Squash merges.
- **CI:** GitHub Actions. Cargo test, vitest, format preservation corpus run on every PR.
- **Releases:** Semver. Tagged releases with binaries for macOS (Intel + ARM), Windows, Linux.
- **Issue templates:** Bug, feature, format-preservation regression. The format-preservation regression template is non-negotiable; it asks for a minimal reproducer .md file.
- **Code of conduct:** Contributor Covenant 2.1.
- **Contributing:** README explains the architecture, the format preservation contract, and the philosophy. Anyone proposing a feature that pretty-prints the file gets a polite no.

### Repository layout (initial)

```
vellum/
├── crates/
│   ├── vellum-core/        Rust core (parser, runtime, fs, MCP)
│   ├── vellum-app/         Tauri shell entry
│   └── vellum-corpus/      Format preservation test corpus + runner
├── ui/
│   ├── src/                React + ProseMirror + CodeMirror
│   └── package.json
├── docs/
│   ├── architecture.md
│   ├── format-preservation.md
│   └── trust.md
├── .github/
│   └── workflows/
├── Cargo.toml
├── package.json
├── README.md
├── LICENSE
├── CODE_OF_CONDUCT.md
└── CONTRIBUTING.md
```

---

## Known risks accepted

Going into scaffolding, here are the risks we know about and are choosing to live with:

1. **ProseMirror ↔ block-source patch architecture is novel for us.** The toggle-time sync model (not per-keystroke) plus PM-decorated stable block IDs (see §Block identity) is the simplification that makes this tractable. Risk: subtle bugs at the source/rendered boundary, especially when the identity sidecar is absent. Mitigation: format preservation corpus runs at both steady-state and cold-state gates; the rest get found in real use.
2. **MD block parser with byte-range preservation depends on `pulldown-cmark` `OffsetIter`.** The block-boundary reconstruction algorithm is specified in §Block boundary reconstruction. Risk: irreducible edge cases (HTML blocks, lazy continuation, link reference definitions) may force a fallback to `markdown-rs`. Mitigation: corpus surfaces this within Gate 30A; fallback path is documented.
3. **Trust toml may not scale to many tools.** Probably fine until I have 50+ MCP servers configured, which won't be soon. Mitigation: revisit if it becomes painful.
4. **Single-user constraint may not survive contact with reality.** If others want to use this and start filing issues, the design may need to harden in places we deliberately simplified (multi-machine grants, shared trust). Mitigation: we'll learn from the issue tracker; that's why it's open source.
5. **`pinned` cache fills up over time.** No eviction policy in v1. Mitigation: settings panel surfaces cache size; manual clear; revisit.
6. **Doctor cut from v1.** Inline validation covers the common cases. Risk: validation gaps surface at unhappy moments. Mitigation: keep an eye on what classes of bugs show up; build doctor only when there's a clear pattern.
7. **MCP server identity binding is best-effort.** The `bind` field constrains trust to (advertised name + invocation command/URL) pairs, which raises the spoofing bar but doesn't eliminate it. A user who configures a server with `bind: *` opts out of protection; a user who lets a wildcard server pattern match too broadly is also exposed. Mitigation: the manifest discourages `bind: *` outside `[defaults]`, and the settings panel highlights any server entry resolving to a wildcard.
8. **v1.0 ships unsigned/ad-hoc-signed.** macOS notarization, Windows code-signing, and Linux signature-distribution are deferred to v1.1. Risk: macOS users see the unidentified-developer warning on first launch; Windows users see SmartScreen warnings. Mitigation: README documents the workaround; v1.1 backlog item exists.

---

## Out of scope for v0.3, on the v1.5+ roadmap

- iOS read+edit viewer (v1.5 priority 1).
- `vellum:agent`, `vellum:transform`, `vellum:include`.
- Tantivy global search UI.
- Multiplayer (Yjs or Automerge).
- Evidence bundle export.
- Plugin marketplace.
- Encryption at rest for cache.
- Built-in git integration.
- Chart renderer.
- Show-as-of-run-X historical render.
- Structural table diff.
- Renderer plugin manifest (the contract is internal-only in v1).

---

## Glossary

- **Doc** — a single `.md` file managed by Vellum.
- **Vault** — a directory tree of docs sharing a `.vellum/` config.
- **Primitive** — a fenced code block with `vellum:*` language tag.
- **Snapshot** — a recorded result of a primitive run, sidecar (`pinned`) or inline (`inline_snapshot`).
- **Evidence State** — the user-facing status of a rendered primitive (Live / Cached / Frozen / Changed-since-frozen / Broken / Untrusted).
- **Source-preserving save** — the guarantee that Vellum does not modify file bytes outside what the user explicitly edited.
- **Format preservation corpus** — the test corpus that gates editor work; v1 must pass it byte-identical.
- **Run log** — append-only NDJSON record of every primitive execution.
- **Lossless duality** — rendered and source views represent the same document with no information loss.

---

## Sign-off

This is the locked spec for v1 scaffolding. Three internal cycles + two external multi-model rounds (Codex + Claude -p) applied. All Critical and High findings resolved. Code reveals what specs cannot. Next move: scaffold the repo, build the format preservation corpus first, then the editor core.
