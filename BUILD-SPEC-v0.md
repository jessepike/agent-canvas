# AgentCanvas v0 — Build Spec

**Audience:** Codex (implementer). Self-contained — Codex has no conversation context.
**Owner:** Jesse. **Orchestrator:** Claude (CPO).
**Build window:** One session, end-to-end. Layer A discipline.
**Date:** 2026-05-19.
**Status:** Revised after Codex architecture review (Critical/High findings integrated).

## Codex Review Findings — Integrated

Codex pressure-tested the original v0 plan and surfaced these:

| Severity | Finding | Resolution in this spec |
|----------|---------|--------------------------|
| Critical | Full 10-tool MCP + comments + rendered PM editing = too broad for one session | MCP live integration DEFERRED to v0.2. Pasteboard handoff for Send-to-Claude in v0. Comments DEFERRED. Source-only editing in v0. |
| Critical | LWW + banner insufficient for concurrent edits | Optimistic concurrency with `base_hash` on every save. Mismatch = conflict state, not overwrite. (Already in Vellum's atomic-save code — carry forward.) |
| High | iCloud watcher unreliable as primary round-trip | Watcher = UI invalidation hint. Correctness from stat+hash on save. Add rescan-on-focus + rescan-before-open. |
| High | Comment anchors need spec or cut | CUT from v0. Revisit in v0.2 with quote+context+hash anchoring. |
| High | Persona registry hard-coupled to one path | Path configurable, default = `~/code/_shared/pike-agents/plugins/`, cache in SQLite, graceful fallback to built-in default table if absent. |
| High | Tauri UI carry-forward likely slower than fresh | Carry forward Rust substrate (atomic save, parser, corpus, watcher). FRESH-START the React shell from prototypes. |
| High | State schema path-only strands on rename | Track `{path, last_seen_hash, size, mtime}` as artifact identity. Re-link on rename via hash match. |
| Low | `s` send-to-claude key implies live MCP | v0 = pasteboard handoff (`pbcopy` formatted payload). Live MCP in v0.2. |

---

## Purpose

Build the v0 AgentCanvas desktop application — a Mac-native (Tauri) workbench for viewing, lightly editing, and round-tripping artifacts that LLM agents produce. This is the SUCCESSOR to Vellum v1.0 (the locked spec at `vellum-spec-v0.3.md` is SUPERSEDED — read for what carries forward, not for what to build).

This single session ships an end-to-end useful tool. Not a stub. Not a scaffold-only. A working application Jesse can open, point at his iCloud folder, see his agent-produced artifacts, edit them, and round-trip with terminal-based Claude Code / Codex sessions.

---

## Reads-Required (in this order, before writing any code)

1. **`intent.md`** — v2.0 destination
2. **`prototypes/visual-system.md`** — canonical design tokens (typography, colors, shadows)
3. **`prototypes/index.html`** — open and review each of A, B, C, D, E, F, I, K to understand the visual + interaction target. These ARE the spec for what the UI looks and feels like.
4. **`vellum-spec-v0.3.md`** — the SUPERSEDED Vellum spec. Read sections on:
   - Block boundary reconstruction (parser)
   - Atomic save with stat+hash corruption guard
   - Format-preservation corpus
   - ProseMirror integration
   These pieces transplant. Everything else in that spec (live-query primitives, MCP-as-block-execution-trust, evidence state, runs.ndjson) is DROPPED.
5. **`CLAUDE.md`** — project context (read but expect to update — see Migration Tasks)
6. **`decisions.md`** — current commitments
7. **The Codex review at `/tmp/codex-agentcanvas-review.jsonl`** — read after it lands. Integrate any Critical or High findings into your implementation plan before writing code. If not yet present when you start, proceed and integrate findings on next pass.

---

## Architecture Rules (Invariants)

These are NOT optional. They're the spine of the product.

### A1 — File substrate is iCloud Drive at a fixed path

```
~/iCloud/AgentCanvas/
├── Inbox/
│   └── captures/
├── Projects/
│   └── {ProjectName}/
└── Archive/
```

The viewer ONLY shows files in this tree. Agents drop artifacts to `Inbox/`. Filing moves files from `Inbox/*` to `Projects/{Name}/`. Other Markdown files on the user's Mac are NOT touched, NOT indexed, NOT shown.

If the folder doesn't exist on first launch, the app creates it with sensible defaults (`Projects/Default/`, empty `Inbox/`).

**Detection:** macOS iCloud Drive lives under `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/` — but the user-facing path is `~/iCloud/AgentCanvas/`. Use the actual iCloud path; the `~/iCloud` shortcut may not exist. Auto-create a symlink for convenience if it doesn't exist.

### A2 — Files stay plain. Edits preserve byte-level source.

Carry forward Vellum's atomic-save guard: stat+hash the file before writing, abort to "file changed on disk" banner if the on-disk hash differs from the base. Carry forward the format-preservation parser.

When the user edits one paragraph, only that paragraph's bytes change. No reformat-on-save, no whitespace normalization.

### A3 — Viewer state lives in a sidecar SQLite

```
~/Library/Application Support/AgentCanvas/state.db
```

Holds: read state, pin tags, comment anchors, last-viewed timestamps, agent session history. Per-machine. Not synced via iCloud (sync later if it becomes a real need).

Tables (suggested):
- `files (path PK, last_read_at, pinned, archived)`
- `agent_sessions (id PK, persona, backbone, context, connected_at, last_active)`
- `comments (id PK, file_path, anchor_text, anchor_offset, author, body, thread_id, resolved, created_at)`
- `pending_edits (id PK, file_path, proposer, diff, reasoning, created_at, status)`

### A4 — Send-to-Claude via pasteboard handoff (v0). MCP deferred to v0.2.

For v0, "Send to Claude" copies a formatted payload to the clipboard via `pbcopy`:

```
Path: ~/iCloud/AgentCanvas/Inbox/agf-positioning-v3.md
Project: AGRC
Persona inferred: cto·claude

[file contents pasted here]

— Jesse's note: {optional}
```

The user then pastes into their active Claude Code / Claude Desktop / Codex terminal. This works today, requires zero protocol surface, and validates the loop.

**MCP server with 10 tools is v0.2.** When we revisit, the planned tool surface is: `list_artifacts`, `get_artifact`, `propose_edit`, `commit_edit`, `add_comment`, `get_comments`, `get_current_focus`, `notify_user`, `get_user_messages`, `attach_artifact`. With persona/backbone/context announcement on `initialize`. Don't build this now.

For v0, the agent panel is populated MANUALLY: a small "Add agent session" affordance lets Jesse declare what's currently running ("cto·claude [AGRC]"). State persists in `agent_sessions` table. Empty state collapses panel to a thin gutter with "+ Connect" affordance.

### A5 — Agent persona colors are sourced from pike-agents config (with graceful fallback)

Configurable path. Default:

```
~/code/_shared/pike-agents/plugins/{persona}/agents/{persona}.md
```

Each persona's `color:` frontmatter field is canonical. Read these files at viewer startup to build a persona registry. **Cache the resolved registry in SQLite (`personas` table).** If the configured path doesn't exist (e.g., on a fresh Mac without pike-agents), fall back to a built-in default table — DO NOT fail viewer startup. Log a warning in the UI ("persona registry unavailable, using defaults").

```
cpo            → blue
cto            → indigo
cfo            → green
cro            → orange
cmo            → purple
ciso           → red
krypton        → magenta
forge          → amber
agf-architect  → teal
```

Map semantic color names to specific hex values (define in `visual-system.md` — append a `--persona-{name}` token block). Generic `claude` and `codex` (no persona) get a neutral treatment defined in visual-system.md.

If pike-agents config is unavailable (user without that repo), fall back to a built-in default persona table.

### A6 — Build target: Tauri 2 + Rust + React + ProseMirror + CodeMirror

Carry forward:
- The Rust crate `vellum-app` (rename to `agent-canvas-app`)
- Atomic-save guard in Rust
- Markdown parser in Rust
- Format-preservation corpus tests
- ProseMirror integration for Markdown editing
- CodeMirror for source view

Drop everything Vellum-specific that doesn't transplant (live-query primitives, trust.toml model, evidence state).

### A7 — Visual system is non-negotiable

`prototypes/visual-system.md` defines every color, font, shadow. Use these as CSS custom properties. Do not invent new colors. If a state genuinely needs a color not present, propose it as an addition to visual-system.md first (commit the addition, then use it).

Typography:
- **Funnel Sans** — UI chrome (load from Google Fonts)
- **Funnel Display** — display titles
- **Newsreader** — prose / serif personas (italic for persona badges)
- **JetBrains Mono** — code, mono backbone tags

### A8 — Optimistic concurrency: every save carries base_hash; mismatch = conflict state

When a file is opened, record its `base_hash` (BLAKE3 or SHA-256 of full contents).

When the user saves:
1. Re-read the file from disk
2. Compute current on-disk hash
3. If current hash ≠ base_hash → abort save, transition to **conflict state**, show banner:
   > "{filename} changed on disk since open. Save aborted — reload or copy your edit elsewhere."
4. If hashes match → write to temp file, atomic rename via `rename(2)`, update base_hash to new value

Three-way merge UI is v0.2. v0 = abort + banner is the safe minimum.

This is NOT last-write-wins. The whole point is that two concurrent edits NEVER silently overwrite each other.

### A9 — Comments deferred to v0.2

Comments are cut from v0. The anchoring problem (line numbers fragile, PM positions ephemeral, quote anchors need careful design) needs proper spec work. Don't ship a half-baked comment system.

When we revisit in v0.2, the planned anchor model is:
```
{file_path, base_hash, byte_range, selected_text, prefix, suffix, text_hash, created_line}
```
Re-anchoring strategy: exact quote match first → context (prefix/suffix) match → line fallback with "stale anchor" warning. Document this in v0.2 spec when we get there.

### A10 — Artifact identity by path + content history (not path alone)

State schema MUST track artifact identity beyond raw path. Minimum:
```sql
files (
  path TEXT PRIMARY KEY,
  last_seen_hash TEXT NOT NULL,
  size INTEGER,
  mtime INTEGER,
  pinned BOOLEAN DEFAULT 0,
  archived BOOLEAN DEFAULT 0,
  last_read_at INTEGER
)
```

When a file is renamed (path changes, content hash matches a known previous file), update the record's path rather than orphaning state. This is a Slice-2 implementation detail but a Slice-1 invariant.

### A11 — Watcher is a UI invalidation hint, not a correctness source

iCloud file-watching is unreliable: events delayed, coalesced, duplicated, sometimes missed during sync. Treat it as a hint that *something might have changed* — re-list the inbox, re-stat displayed files.

Correctness comes from stat+hash on every save. Belt-and-suspenders:
- File-watcher fires → re-list inbox + re-stat current file
- Window focus → re-list inbox + re-stat current file
- Before opening any file → fresh read
- Before saving → fresh stat+hash check (A8)

---

## Build Sequence (revised after Codex review)

Execute in this order. Atomic commits at each completed slice (conventional commits, `type(scope): description`).

### Slice 1 — Migration scaffolding (~30 min)

1. Rename `crates/vellum-app/` → `crates/agent-canvas-app/`. Update `Cargo.toml` package name and all internal references. Update `tauri.conf.json` (productName="AgentCanvas", identifier="dev.jessepike.agentcanvas").
2. Rename UI package name in `ui/package.json`. Window title, app title, etc.
3. Create `legacy/` directory. Move `vellum-spec-v0.3.md`, `review-findings.md`, and any other Vellum-v1-only docs into `legacy/`.
4. **FRESH-START the React UI shell** per Codex's High finding — old Vellum UI was source/rendered editor, AgentCanvas is artifact inbox. Keep `ui/src/ipc.ts` and Rust-side bindings; replace `App.tsx` and components with fresh code from prototypes.
5. Update `CLAUDE.md` to reflect AgentCanvas v0 (use `intent.md` v2.0 as the source). Drop Vellum-specific language. Reference BUILD-SPEC-v0.md, intent v2.0, visual-system.md.
6. Update `BACKLOG.md` to AgentCanvas v0 scope (drop Vellum P0/30A/30B; replace with v0 tasks from this build spec).
7. Append a decision to `decisions.md`: `D-AC-1 [H-D] Vellum v1.0 abandoned, AgentCanvas v0 begins`.
8. Commit: `feat(rename): migrate Vellum scaffolding to AgentCanvas; fresh UI shell`

### Slice 2 — iCloud substrate + viewer launches against it (~45 min)

1. Implement `~/iCloud/AgentCanvas/` folder bootstrap (create if missing, with `Inbox/captures/`, `Projects/Default/`, `Archive/`).
2. SQLite state DB at `~/Library/Application Support/AgentCanvas/state.db` — schema + open + migrate.
3. Tauri command: `list_inbox()` returns array of file metadata from `~/iCloud/AgentCanvas/Inbox/`.
4. UI: Two-pane layout matches `prototypes/prototype-A-main.html`. Left pane shows Inbox file list (real files from iCloud). Right pane shows placeholder "Select a file."
5. Apply visual-system.md tokens as CSS custom properties.
6. Commit: `feat(substrate): wire iCloud folder + SQLite state + inbox list view`

### Slice 3 — Markdown render + atomic save (~45 min)

1. Click a `.md` file in inbox → reads file → renders to right pane with ProseMirror.
2. Edit mode toggle (pencil icon) — toggle between rendered-and-editable. Default = view (Apple Notes pattern).
3. Save via existing Rust atomic-save guard (stat+hash before rename). On hash mismatch, show banner per A8.
4. Visible "Saved HH:MM:SS" toast on successful save.
5. Re-render rendered view after save.
6. Commit: `feat(viewer): markdown render + edit mode + atomic save`

### Slice 4 — HTML render (sandboxed iframe) (~30 min)

1. Click a `.html` file → render in sandboxed iframe with appropriate sandbox attributes (`sandbox="allow-same-origin"` — no script execution unless explicitly toggled by user; v0 = scripts off).
2. Toolbar: "View Source" toggle (shows raw HTML), "Send to Claude" button.
3. Commit: `feat(viewer): sandboxed HTML rendering`

### Slice 5 — File-watcher round-trip (~30 min)

1. Use Rust `notify` crate to watch `~/iCloud/AgentCanvas/` recursively.
2. On file-change events, debounce (~200ms) and emit a Tauri event to the UI.
3. UI re-loads file list + re-renders current file if changed.
4. New files arriving in `Inbox/` get the "just arrived" highlight (CSS animation + blue dot).
5. Test: write a file to `Inbox/` from a terminal → viewer shows it appear within 1 second.
6. Commit: `feat(watcher): live file-watch for inbox round-trip`

### Slice 6 — Persona registry + agent badges (~30 min)

1. Read pike-agents plugin frontmatter at startup. Build persona registry: `{name, color, display_label}`.
2. Files get an inferred persona badge — for v0, infer from file metadata if available; otherwise default to a generic `claude` badge. (Real persona attribution comes via MCP in Slice 7.)
3. Apply persona color tokens (define `--persona-cpo: #1F5BD8` etc. — pick hex values matching the semantic terminal colors).
4. Commit: `feat(personas): wire persona registry from pike-agents config`

### Slice 7 — Pasteboard handoff "Send to Claude" (~20 min)

1. Tauri command: `send_to_clipboard(payload: SendPayload)` writes formatted text via `arboard` or shell `pbcopy`.
2. UI: "Send to Claude" button + ⌘⏎ shortcut. Builds the payload (path, project, persona inferred, file contents, optional note) and copies.
3. Toast confirms: "Copied to clipboard — paste into your Claude / Codex session"
4. Commit: `feat(handoff): pasteboard send-to-claude payload`

### Slice 8 — Agent panel (manually-seeded, no live MCP) (~25 min)

1. Right-side ~280px panel showing declared agent sessions (from `agent_sessions` table — manually populated in v0).
2. "+ Add session" affordance opens a small form: persona dropdown (from registry), backbone (claude/codex/etc.), context (free text or pick from project list). Saves to table.
3. Cards: persona badge (Variant C Persona-typed per `prototype-E-agent-panel.html`), backbone tag (mono pill), bracketed context.
4. Empty state: collapse panel to a thin gutter with "+ Connect" affordance (per friction-log feedback).
5. Commit: `feat(ui): agent panel with manual session declaration`

**Note:** Live MCP server is v0.2. Don't build a socket / protocol surface today.

### Slice 9 — Command palette ⌘K (~45 min)

1. Vanilla React + Cmd+K keyboard binding.
2. Sections: ACTIONS, FILES, COMMANDS per `prototypes/prototype-I-command-palette.html`.
3. Real keyboard wiring: ↑↓ navigate, ⏎ fire (real actions where wired, console.log + toast for stubs), Esc dismiss.
4. Typeahead filtering across actions + files.
5. Most-important actions wired for real: Send to Claude (writes user message to outbox), Toggle Pin, Archive, Open Project.
6. Commit: `feat(ui): cmd-k command palette with real keyboard wiring`

### Slice 10 — Project mode + mode toggle (~30 min)

1. Two-column inbox mode (default) ↔ three-column project mode (when a project folder is selected in sidebar).
2. Match `prototypes/prototype-F-project-mode.html`.
3. Right pane auto-populates with most-recent file when entering project mode.
4. Commit: `feat(ui): two-column inbox + three-column project mode toggle`

### Slice 11 — Polish + smoke test (~25 min)

1. Keyboard-first bindings from `prototypes/prototype-K-keyboard-first.html`: j/k nav, e for edit-mode, s for send-to-claude (pasteboard), p for pin, ⌘⌫ archive, / focus search, ? toggle shortcuts overlay.
2. Rescan-on-focus: when the window gains focus, re-list inbox + re-stat current file (Codex high finding).
3. End-to-end smoke test:
   - Open viewer cold → iCloud folder bootstrapped → empty inbox shown
   - From terminal: `echo "# Test artifact" > ~/iCloud/AgentCanvas/Inbox/test.md`
   - Viewer shows test.md appear within 1-2 seconds (watcher hint) OR on next window focus (rescan)
   - Click test.md → renders → toggle edit mode → edit → save → toast shows
   - Modify file externally → try to save again → "changed on disk" banner appears, save aborts
   - Add a manual agent session in the agent panel → cto·claude [AGRC] appears
   - Click "Send to Claude" → pasteboard contains formatted payload
4. Commit: `feat(polish): keyboard bindings + rescan-on-focus + smoke test`

### Slice 12 — README + LICENSE + push (~15 min)

1. Write `README.md`: what AgentCanvas is, screenshots from `prototypes/`, install instructions, Apache 2.0 notice.
2. Add `LICENSE` (Apache 2.0).
3. Update `status.md`: "v0 shipped 2026-05-19. {tasks_count} commits. Working end-to-end."
4. Commit: `chore(release): v0.1.0 — first usable AgentCanvas`
5. Tag: `v0.1.0`.

---

## Acceptance Criteria (How We Know v0 is Done)

These must all pass:

1. **Cold-start:** app launches, bootstraps iCloud folder, shows empty Inbox (or existing files).
2. **File round-trip:** write a Markdown file to `~/iCloud/AgentCanvas/Inbox/foo.md` from a terminal — viewer shows it appear within 1-2 seconds (via watcher) OR on next window focus (via rescan).
3. **Markdown render:** click `.md` file → rendered preview, looking like `prototype-A-main.html`.
4. **HTML render:** click `.html` file → rendered in sandboxed iframe (scripts disabled by default) with full visual fidelity.
5. **Source edit + atomic save:** toggle edit mode → CodeMirror source view → make a small edit → save → file on disk reflects edit byte-for-byte (no reformat).
6. **Optimistic concurrency:** modify file externally → try to save in viewer → "changed on disk" banner appears, save aborts (no silent overwrite).
7. **Agent panel:** add a manual session → cto·claude [AGRC] card appears with correct persona color from registry. Empty state collapses to gutter.
8. **Pasteboard handoff:** click "Send to Claude" or press ⌘⏎ → formatted payload in clipboard → paste into terminal shows it.
9. **Command palette:** ⌘K opens, type "send", press ⏎ → action fires.
10. **Keyboard nav:** j/k moves selection in inbox, ⏎ opens file, p pins, e edit-mode.
11. **Project mode:** click project folder → layout switches to three-column.
12. **Persona colors:** badges use colors from pike-agents frontmatter (or built-in fallback if pike-agents repo absent — no startup failure).
13. **Rescan on focus:** switch away from viewer, modify a file externally, switch back → viewer reflects current state.

---

## Out of Scope for v0 (Deferred to v0.2)

The list below is the v0.1.0 deferral set. **v0.2.0 (2026-05-20) landed everything in `docs/BUILD-SPEC-v0.2-finish.md` except live MCP.** Items now in scope are struck through; the remaining v0.2-proper deferral is **Live MCP server only**.

- **Live MCP server with 10 tools** — pasteboard handoff is the v0 substitute (still deferred — v0.2-proper target)
- ~~Comments / anchors~~ — shipped in v0.2.0 Slice 6 (raw-source-offset anchors persisted in sidecar)
- ~~Rendered ProseMirror editing~~ — shipped in v0.2.0 Slice 5e (rendered edit with source-preserving save + source fallback)
- ~~Three-way merge UI~~ — shipped in v0.2.0 Slice 5g (base_snapshot in sidecar + per-block resolve dialog)
- ~~Annotation toolbar~~ — shipped in v0.2.0 Slice 5f (Bold / Italic / Strike / Code / Mark-for-Revision)
- ~~PNG / JSON / TXT viewer modes~~ — shipped in v0.2.0 Slice 5a–c (+ PDF in 5d)
- ~~Search across files~~ — shipped in v0.2.0 Slice 2b (filename substring + ⌘F focus, per-mode)
- Pending Reviews aggregate view — still deferred (per-artifact review state shipped, no aggregate panel yet)
- Cross-machine sync of state.db — still deferred
- iOS reader — still deferred
- Notarization / code-signing — still deferred (v0.2.0 ships ad-hoc/dev-signed)
- Trust boundaries / per-artifact agent visibility — still deferred

---

## Constraints

- **Atomic commits at each Slice.** Conventional commits (`type(scope): description`).
- **No PRs.** Direct push to main.
- **Apache 2.0 from day 1.** Public repo.
- **Read pike-agents frontmatter only.** Do NOT modify anything under `~/code/_shared/pike-agents/` — read-only.
- **No new colors without updating visual-system.md.** If you must add one, commit the addition first.
- **Layer A discipline.** Ship the full loop in one session. Do not stub the MCP server. Do not stub the file-watcher. The whole thing must work end-to-end before you stop.
- **Test smoke conditions in #11 yourself.** Do not declare done without verifying.

---

## Migration Notes for the Repo Rename

The directory rename `~/code/sandbox/vellum/` → `~/code/sandbox/agent-canvas/` and the GitHub repo rename (`jessepike/vellum` → `jessepike/agent-canvas`) will happen AFTER you complete the build (Jesse will run them, or Claude will). Build in-place at `~/code/sandbox/vellum/`. Do not move the directory yourself.

However, internal package names + identifiers SHOULD be updated in Slice 1 (Cargo.toml package name, Tauri productName/identifier, package.json name).

---

## Reporting

After each slice: brief summary in your message + the commit hash. After all slices: summary of what shipped vs spec, any deferrals or in-flight concerns, and the smoke-test results.

If you hit a Critical blocker (something in the architecture is structurally broken, not a small bug), STOP and report. Don't keep grinding on a wrong premise.

---

## Where to Find Things

| Need | Location |
|------|----------|
| Product intent | `intent.md` v2.0 |
| Visual system | `prototypes/visual-system.md` |
| UI prototypes | `prototypes/index.html` + HTML files |
| Vellum carry-forward | `vellum-spec-v0.3.md` (parser, atomic save, corpus only) |
| Architecture review | `/tmp/codex-agentcanvas-review.jsonl` (read after it lands) |
| Persona config | `~/code/_shared/pike-agents/plugins/{name}/agents/{name}.md` |
| Existing Rust crate | `crates/vellum-app/` (rename in Slice 1) |
| Existing UI | `ui/` |
