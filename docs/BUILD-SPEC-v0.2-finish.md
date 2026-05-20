# AgentCanvas v0.2-finish — Build Spec

**Goal:** Land every remaining v0-scope item before starting v0.2 MCP work.
**Scope:** 26 items across correctness, features, viewers, editing, comments, and polish.
**Trigger:** v0.1.1 + 8 fix commits validated by real use 2026-05-19; gap-scan completed.
**Build window:** ~4-6 Codex hours across 6 atomic slices.
**Owner:** Jesse. **Orchestrator:** Claude (CPO). **Implementer:** Codex.
**Out of scope:** Live MCP server (deliberately deferred to v0.2-proper).

---

## Reads-Required (for each Codex slice)

1. This file (`docs/BUILD-SPEC-v0.2-finish.md`)
2. `intent.md` — product north star
3. `CLAUDE.md` — invariants
4. `BUILD-SPEC-v0.md` — original v0 architecture
5. `docs/PATCH-SPEC-v0.1.1.md` — v0.1.1 context
6. `docs/active/codex-gap-scan-2026-05-19.md` — gap report
7. `prototypes/visual-system.md` — authoritative tokens
8. `crates/agent-canvas-app/src/main.rs` — Rust backend
9. `ui/src/App.tsx`, `ui/src/ipc.ts`, `ui/src/styles.css` — UI

---

## Architecture Invariants (do NOT violate)

Carrying forward A1-A11 from BUILD-SPEC-v0.md plus:

- **A12 — Path-bounding:** Every Rust command that takes a path parameter MUST verify the canonicalized path lives under `state.paths.canvas_root`. Reject with `Err` otherwise.
- **A13 — Artifact identity by (path, hash) tuple, not hash alone:** Same-content duplicates do NOT share state. Identity-by-hash relink only triggers when the old path no longer exists on disk (rename, not duplication).
- **A14 — Persona color is registry-derived:** UI must consume `Persona.color` from the registry, never hard-coded badge classes. Adding a new persona = drop a frontmatter file; no CSS edit required.
- **A15 — Visual-system tokens are authoritative:** Any color used in `ui/src/styles.css` must be defined in `prototypes/visual-system.md` first.
- **A16 — Single window chrome:** App uses the real macOS chrome only. No CSS-faked traffic lights / fake titlebar inside the Tauri window.
- **A17 — In-app modals, not `window.prompt`:** Conflict resolution, default-agent picker, file rename — all use in-app dialog components.

---

## Slice 1 — Correctness foundation (~45 min)

The four real bugs Codex found. Land first — everything else builds on a safer substrate.

### 1a. Path-bounding on all document/sidecar commands

Add a `path_within_canvas(state, path)` helper in Rust that:
- Canonicalizes the input path
- Compares to `state.paths.canvas_root.canonicalize()`
- Returns `Err("path outside AgentCanvas: ...")` if not a descendant

Apply to: `open_document`, `write_document`, `load_sidecar`, `save_sidecar`, `parse_document` (for symmetry, even though it doesn't touch disk), `archive_file`, `toggle_pin`, `delete_file`, `reveal_in_finder`, `move_file_to_project`, `move_file_to_archive`. Any new command that takes a `path: String` parameter.

Unit test: `path_within_canvas("/etc/passwd")` → `Err`. `path_within_canvas("<canvas_root>/Inbox/x.md")` → `Ok`. Symlinks resolve before comparison.

### 1b. Artifact identity tightening

In `upsert_file_state` (main.rs:856): only do hash-relink when the matched `existing_path` no longer exists on disk. This converts "two different files, same content" from a state-merge bug into independent rows. Rename (path A no longer exists, path B has identical hash) still relinks correctly.

Unit test: create two files with identical content, pin one, verify the other is NOT pinned after rescan.

### 1c. Bootstrap error handling

Replace `bootstrap().expect(...)` at main.rs:629 and the `tauri::generate_handler!` setup at :1228 with a fallible path. On failure: show a modal error inside the app window with the actionable message (e.g., "Could not access iCloud Drive at <path>. Open System Settings → iCloud Drive to enable. [Retry] [Open Finder]"). No panic.

### 1d. `last_read_at` is actually written

When `open_document` succeeds, UPDATE `files SET last_read_at = strftime('%s','now') WHERE path = ?1`. Hydrate as before. Display as "Last opened" timestamp in file context menu (read-only metadata line).

**Commit:** `fix(correctness): path-bound commands, tighten artifact identity, fallible bootstrap, write last_read_at`

---

## Slice 2 — Sidebar + window chrome (~60 min)

### 2a. Drop the fake window chrome

Delete from `ui/src/App.tsx`: `.desktop`, `.window-shell`, `.titlebar`, `.traffic-lights`, `.titlebar-title`, `.titlebar-action`, `.titlebar-open` wrapper divs. Content fills the real Tauri window.

Delete from `ui/src/styles.css`: all `.desktop`, `.window-shell`, `.titlebar`, `.tl`, `.tl-*`, `.traffic-lights`, `.titlebar-title`, `.titlebar-action`, `.titlebar-open` rules.

Move "Rescan" + "+" (new file) buttons into the per-artifact toolbar (next to Edit / Send to Agent / Save). They become global toolbar actions visible whether or not an artifact is open.

### 2b. Search actually filters

Wire the existing search input. Behavior:
- Filters the currently-visible file list (inbox / project / archive / pinned) in real time
- Fuzzy match on filename (simple substring case-insensitive is fine for v0; if a fuzzy lib is already a Rust dep, use it)
- Empty input shows full list
- Esc clears
- ⌘F focuses the search input
- Persists per-mode (switching modes resets)

### 2c. Project counts in sidebar

Replace hard-coded `0` at App.tsx:894 with a real count. Add a new Tauri command `list_project_counts() -> HashMap<String, usize>` that scans `Projects/` and returns artifact count per project. Fetched in `refresh()`. UI reads from the map.

### 2d. Project rename / delete in sidebar context menu

Right-click on a project row in the sidebar → context menu with:
- Open
- Rename... (in-app dialog; renames the folder + updates DB paths)
- Delete... (confirmation; deletes the folder; archives all contained files first OR refuses if folder has files — pick refuse for v0 safety, with message "Move files out before deleting project")

New Tauri commands: `rename_project(old: String, new: String)`, `delete_project_if_empty(name: String)`.

### 2e. Persona registry refresh action

New Tauri command `reload_persona_registry()` that re-reads `~/code/_shared/pike-agents/plugins/**/agents/*.md` frontmatter. New command-palette entry "Reload Persona Registry". UI calls it + re-renders badges. No app restart required.

**Commit:** `feat(sidebar): drop window-in-window chrome, real search/filter, project counts/rename/delete, persona reload`

---

## Slice 3 — Persona system + visual-system reconciliation (~60 min)

### 3a. Persona colors registry-driven

Remove hard-coded `.badge-cpo`, `.badge-cto`, etc. classes from `ui/src/styles.css`. Replace with inline `style={{ color: persona.color }}` on the badge component, reading from the registry response.

For italic / mono / styling that's currently per-persona (e.g., `.badge-cto` has italic font), add a `style:` frontmatter field to pike-agents persona docs — optional, falls back to default style. Document in BUILD-SPEC. Skip if scope creep; just keep all badges visually uniform except for color.

### 3b. Persona detection actually reads frontmatter

In `metadata_for_file()` (main.rs:821): if the file is `.md` or `.markdown`, parse the first 4KB looking for YAML frontmatter (`^---\n...---\n`). Look for `persona:`, `author:`, or `agent:` keys in that order. Use the matched value as the persona. Validate against registry; unknown → fall back to `default_persona`.

Cache result by (path, mtime, size) so we don't re-parse on every list.

### 3c. Visual-system token reconciliation

Add to `prototypes/visual-system.md`:
- `--persona-*` token list (mirror what's currently in styles.css plus document the registry-derived flow)
- The button gradient tokens (`#ffffff → #f5f3ec`) as `--btn-bg-top` / `--btn-bg-bot`
- The traffic-light colors (delete from CSS once Slice 2a lands — no longer used)
- The accent-deep gradient (`#3270e8 → #1f4ecc`) as `--btn-primary-top` / `--btn-primary-bot`
- The overlay rgba values as `--overlay-modal-bg` / `--overlay-popover-bg`

Then in `ui/src/styles.css`: replace every raw color literal with a token reference.

**Commit:** `feat(personas): registry-driven colors, frontmatter detection, visual-system token reconciliation`

---

## Slice 4 — File operations (~75 min)

### 4a. File rename

New Tauri command `rename_file(old_path: String, new_name: String) -> FileMetadata`:
- Path-bound (A12)
- Resolves new path = same parent + new_name
- Refuses if new_name contains `/` or `..`
- Refuses if new name already exists (let UI handle conflict modal — see 4d)
- `fs::rename` + update DB row (path field)

Context menu entry "Rename..." → in-app dialog with text input prefilled, Enter confirms, Esc cancels.

Keyboard: ⌘R (with file selected) opens rename dialog. (Conflicts with browser refresh in Tauri? Use F2 instead if so — picks ⌘R first, fallback F2.)

### 4b. Multi-file selection

Sidebar (inbox + project + archive + pinned views) supports:
- Click = select one (existing behavior)
- ⌘-click = toggle in/out of selection set
- Shift-click = range select from anchor to clicked
- Selection set highlighted with a stronger background (use existing `.selected` styling, add `.multi-selected` variant)
- Esc clears selection

When selection size > 1, the artifact viewer shows a "N files selected" placeholder with bulk-action buttons (Send to Agent, Archive, Move to Project, Delete).

### 4c. Multi-file Send

When Send is invoked with N > 1 selected files:
- Send popover shows "Send N files to {agent}"
- Payload format:
  ```
  I'm sending you {N} files from my AgentCanvas.

  My note: {note}

  ---

  File 1 of N: `{relative_path}`
  ```{lang}
  {contents}
  ```

  ---

  File 2 of N: `{relative_path}`
  ```{lang}
  {contents}
  ```

  ...

  Action: {verb}
  ```
- Same action-verb picker and note input
- Single clipboard write; toast says "Copied N files to clipboard for {agent}"

### 4d. In-app conflict modal (replaces `window.prompt`)

New `<ConflictDialog>` React component:
- Header: "Replace {filename}?"
- Body: short prose "A file with this name already exists in {target}"
- Buttons: Replace (red/destructive), Keep Both (auto-renames with `-1`, `-2`, ... suffix), Cancel
- Focus-trapped, Esc cancels, Enter = primary action (Replace? probably Cancel as default to be safe — make Keep Both the default)

Replace every `window.prompt` / `window.confirm` call in App.tsx with this dialog (or a variant `<ConfirmDialog>` for non-conflict yes/no flows).

### 4e. Drag-out: AgentCanvas → Finder

HTML5 drag-from-file-row with native file path. Tauri 2 file-drag API:
- On `dragstart`, call new Tauri command `prepare_drag_out(path: String)` which copies the file to a temp location AND returns the temp path. (Tauri can't directly hand the OS a path from drag events in JS; the API path uses native file drag via `tauri-plugin-drag` or similar.)
- If `tauri-plugin-drag` is not already a dep, add it.
- Drop target = Finder window → file copy appears.

Acceptance: drag an inbox file out → Finder shows the same file in the dropped location. Original stays in AgentCanvas (drag-out is COPY, not MOVE — agent-side state stays intact).

**Commit:** `feat(files): rename, multi-select, multi-send, in-app conflict modal, drag-out to Finder`

---

## Slice 5 — Viewers + editing surface (~90 min)

### 5a. PNG viewer

In `App.tsx` artifact-render switch (line ~1019): add `artifact.kind === "png"` branch. Render `<img src={...}>` centered in the content pane with metadata strip below (dimensions, file size, mtime). Use a `convertFileSrc` from `@tauri-apps/api/core` or a `read_image_as_data_url` Tauri command (whichever is cleaner).

Rust: `open_document` extended (or a new `open_binary_document(path) -> { kind, data_url, size, dimensions? }`) for binary types.

### 5b. JSON viewer

`artifact.kind === "json"`: CodeMirror with JSON syntax highlighting + folding. View-only by default (use Edit mode to enable source editing — same flow as markdown).

If JSON is small (<100 keys / <100KB), render a parallel collapsible-tree view (HTML `<details>` is fine — no fancy lib needed). Toggle between source and tree views.

### 5c. TXT / unknown-text viewer

`artifact.kind === "txt"` or unknown extension: CodeMirror with no syntax highlighting, plain text. Same edit/save flow.

### 5d. PDF embed viewer

`artifact.kind === "pdf"`: `<iframe sandbox src={dataUrlOrLocalUrl}>` filling the content pane. macOS WebView renders PDF natively in iframes.

If sandbox blocks rendering, fall back to `<embed type="application/pdf" src="...">` or PDF.js — pick whichever works first; PDF native render preferred for size/speed.

Read-only. Source edit / save disabled for PDF.

### 5e. ProseMirror rendered editing

Currently the Markdown rendered view (`RenderedView` in App.tsx:1021) is read-only. Make it editable when `editMode` is true.

- ProseMirror state initialized from `artifact.blocks` parsed structure
- On edit: produce new source via existing block→source path (carry-forward from Vellum spec at `legacy/vellum-spec-v0.3.md`)
- Save flow unchanged: `save_document` Tauri command with patches, atomic write

The existing source-view (CodeMirror) stays as alternative for source editing. Toolbar adds a "Source / Rendered" toggle (next to Edit toggle).

### 5f. Annotation toolbar

When `editMode` is on AND artifact is markdown rendered view:
- Floating toolbar appears on text selection
- Buttons: Bold, Italic, Strikethrough, Code, Mark-for-Revision (adds a `<mark data-revision>` span; visible in both rendered and source views with yellow highlight)
- Implements via ProseMirror commands

### 5g. Three-way merge UI on conflict

Current behavior: conflict banner says "file changed externally" and forces reload. Replace with a merge dialog:
- Left: in-memory edit (your draft)
- Middle: common ancestor (`base_hash` content from sidecar last-known-good copy)
- Right: on-disk version
- Per-block resolve: keep mine / keep theirs / edit manually
- "Apply Merge" writes the merged result with a new base_hash; "Discard" reloads the disk version

Sidecar needs to keep the last-saved content snapshot keyed by base_hash. Add to existing `.agentcanvas/<filename>.json` sidecar schema.

**Commit:** `feat(viewers): PNG/JSON/TXT/PDF viewers, ProseMirror editing, annotation toolbar, 3-way merge`

---

## Slice 6 — Comments, review state, action templates, focus, polish (~75 min)

### 6a. Inline comments / anchors (sidecar A — extend JSON sidecar)

Extend `.agentcanvas/<filename>.json` sidecar schema:

```json
{
  "identity_map": {...},
  "base_snapshot": {...},
  "comments": [
    {
      "id": "uuid",
      "author": "jesse",
      "created_at": 1715000000,
      "anchor": { "block_id": "uuid", "start_offset": 12, "end_offset": 48 },
      "body": "This part is unclear, please revise",
      "resolved": false
    }
  ]
}
```

UI:
- Select text in rendered view → "Comment" button in annotation toolbar OR ⌘⇧M
- Sidebar of comments on the right of the content pane (toggleable, default collapsed)
- Each comment renders as a margin note with author, time, body, Resolve button
- Anchor highlights yellow when comment is hovered

Persistence: every comment write triggers `save_sidecar`.

### 6b. Pending-review state per artifact

Add column to `files` table: `review_state TEXT DEFAULT 'unread'`. Enum: `unread | reviewed | needs-work | approved`.

UI:
- File row shows colored dot indicating state (blue=unread, gray=reviewed, orange=needs-work, green=approved)
- Right-click → "Mark as..." → submenu of states
- Send-to-Agent automatically sets state to "needs-work" if action verb is Revise/Critique
- Open file sets state from "unread" → "reviewed" on first open (preserves manual states)

### 6c. Action-verb templates

Action verbs in Send popover today are just labels. Add presets that carry context:
- "Review" — append "Review for clarity, completeness, and correctness. Flag anything that needs my attention."
- "Critique" — append "Critique with rigor. Identify weak claims, missing evidence, structural issues."
- "Revise" — append "Revise per my note above. Preserve voice and structure."
- "Expand" — append "Expand on the thin sections. Add depth where the argument is asserted but not supported."
- "Summarize" — append "Summarize in 200 words or fewer. Lead with the answer."
- "Respond to" — append "Draft a response. Keep it under 200 words."
- "Custom" — verbatim, no preset

Add a settings command-palette entry "Edit Action Templates..." → modal with editable list.

Templates persisted in SQLite `settings` table.

### 6d. Empty-state copy

Per-folder empty states (rendered when `files.length === 0`):
- Inbox: "Empty inbox\n{path}\nDrag files here or use ⌘N"
- Pinned: "No pinned artifacts. ⌘P on any file to pin."
- Archive: "Empty archive. Move artifacts here when you're done with them."
- Project: "Empty project. Drag inbox files to this project to organize them."

### 6e. Focus management on overlays

Send popover, context menu, conflict dialog, rename dialog, comment dialog, command palette:
- Each gets `role="dialog"` and `aria-modal="true"`
- On open: focus first interactive element
- On close: focus returns to the element that triggered the open
- Tab cycles within the dialog (focus trap — use a small helper, no library)
- Esc closes

### 6f. Command-palette typeahead for projects

Replace `Open Project` always-opens-projects[0] with: a project-name typeahead. Palette query mode shifts when user types "project:" prefix, OR a separate row per project. Use the latter for v0 — one "Open: {projectName}" palette row per project. Already showing all in palette, just need to wire each row individually instead of one stub.

**Commit:** `feat(workflow): inline comments, pending-review state, action templates, focus management, polish`

---

## Slice 7 — Release (~30 min)

### 7a. Smoke test (manual checklist)

Run through every acceptance criterion below. Document pass/fail in `status.md`.

### 7b. Visual-system audit

`grep -rn "#[0-9a-fA-F]\{3,6\}" ui/src/styles.css` should return ZERO results (all colors via tokens). Token list in `prototypes/visual-system.md` complete.

### 7c. Documentation refresh

- Update `README.md` with v0.2-finish behaviors (new viewers, search, comments, multi-select, drag-out, rename, etc.)
- Update `intent.md` ONLY if a real direction shift surfaced (probably not — defer)
- Update `BUILD-SPEC-v0.md` "Out Of Scope For v0" — strike everything now in scope; "Live MCP server" only remaining out-of-scope item
- Update `status.md` with v0.2-finish completion entry

### 7d. Version bump + tag

- `tauri.conf.json`: `0.1.1` → `0.2.0`
- `crates/agent-canvas-app/Cargo.toml`: same
- `git tag v0.2.0`

**Commit:** `chore(release): v0.2.0 — complete v0 surface (search, viewers, comments, editing, polish) pre-MCP`

---

## Acceptance Criteria (all must pass)

1. Open `/etc/passwd` via crafted IPC call → `Err` returned, file not opened.
2. Create two empty `.md` files with identical content; pin one; rescan; other is NOT pinned.
3. Disconnect iCloud → app shows in-window error modal, not crash.
4. Open any markdown file → `last_read_at` updates in DB.
5. Window has ONE chrome layer (real macOS), no inner traffic lights.
6. Type "test" in search → file list filters to matching filenames.
7. Sidebar Default project shows real artifact count, not `0`.
8. Right-click project → Rename → in-app dialog → folder renames + DB updates.
9. Add new persona file to `~/code/_shared/pike-agents/plugins/` → command palette "Reload Persona Registry" → new persona badge color appears without restart.
10. Color in a markdown frontmatter `persona: cpo` → file row shows CPO blue badge.
11. `grep "#[0-9a-fA-F]" ui/src/styles.css` returns nothing — all colors tokenized.
12. Right-click file → Rename → in-app dialog → file renames on disk + UI updates.
13. ⌘-click 3 files → all three highlighted → click Send → popover says "Send 3 files to {agent}" → clipboard has multi-file payload.
14. Drag inbox file into Finder window → file copied to Finder location, original stays in inbox.
15. Open `.png` → image renders with metadata strip.
16. Open `.json` → CodeMirror with JSON syntax + collapsible tree toggle.
17. Open `.txt` → CodeMirror plain text, editable.
18. Open `.pdf` → embedded viewer renders the document.
19. Edit a markdown file in rendered view → bold a word → save → source preserves bold markdown.
20. Edit a file in two windows; save in one; second window shows 3-way merge dialog (not just banner).
21. Select text → comment → comment persists across reload; sidebar shows it; Resolve clears it.
22. Right-click file → Mark as Approved → green dot appears in file row.
23. Send with action "Review" → payload contains review template text after the contents.
24. Open send popover, then Esc → focus returns to Send button.
25. Command palette → "Open: AGRC" → opens AGRC project specifically (not projects[0]).
26. All 24 acceptance criteria from v0.1.0 + v0.1.1 still pass.

---

## Out of Scope for v0.2-finish

- Live MCP server / socket protocol — explicit v0.2-proper
- Real-time multi-user collaboration
- Cloud sync beyond iCloud-of-files
- Comment threading / @mentions
- Comment notifications
- Custom themes
- Plugins / extensions
- Mobile / iOS

---

## Constraints

- One atomic commit per slice; conventional commits.
- Co-Authored-By: Claude (Planner) + Codex (Implementer) on every commit.
- Direct push to main (no PRs) after each slice when its acceptance criteria pass.
- A12-A17 invariants are NOT negotiable.
- All visual-system additions must update `prototypes/visual-system.md` BEFORE the CSS uses the new token (per CLAUDE.md).
- All NEW Tauri commands MUST be path-bounded if they touch a path.
- All NEW dialogs MUST use the focus-management pattern (Slice 6e).

---

## Sequencing Rules

- Slices land in order 1 → 7. No skipping ahead.
- After each slice commits and acceptance criteria pass, push to origin/main.
- If a slice surfaces a blocker (compile fails, test fails, design gap), pause and surface to owner. Do not patch over by skipping items.
- Codex runs each slice as a separate `codex exec --full-auto` invocation, with this spec as the read-required reference. Claude verifies path-by-path after Codex finishes each slice.
