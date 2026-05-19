# AgentCanvas v0.1.1 — Patch Spec

**Trigger:** Jesse's first real-use friction on v0.1.0 (2026-05-19).
**Scope:** Tight patch. Friction-log → fix → ship same day.
**Build window:** One Codex session, ~30-60 min.
**Owner:** Jesse. **Orchestrator:** Claude (CPO). **Implementer:** Codex.

---

## Reads-Required

1. This file (`docs/PATCH-SPEC-v0.1.1.md`)
2. `BUILD-SPEC-v0.md` — for architecture invariants and v0.1.0 context
3. `CLAUDE.md`, `status.md`, `BACKLOG.md`
4. The existing Rust handoff implementation (search for `pbcopy` / `send_to_clipboard` / handoff payload — likely in `crates/agent-canvas-app/src/`)
5. The existing UI Send-to-Claude button + agent panel components in `ui/src/`

---

## Friction signals to address

**F1 — Send-to-Agent payload is unusable as an agent prompt.**
Current output is metadata + raw file + empty note. Not actionable. Agent receives it and has no idea what Jesse wants.

**F2 — Drag-and-drop missing.**
Jesse tried dragging files (Finder → inbox, inbox → project), expected it to work. Doesn't.

**F3 — "Send to Claude" naming is wrong in multi-agent world.**
Should reflect the actual target (default agent persona·backbone) or be agent-agnostic.

**F4 — Obscure iCloud absolute path in payload.**
`/Users/jessepike/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/Inbox/test.md` is noise. Use `Inbox/test.md` relative to AgentCanvas root.

---

## Fixes

### Fix 1 — Restructured Send-to-Agent payload

**New payload format** (single string, copied to clipboard):

```
I'm sending you `{relative_path}` from my AgentCanvas.

{If note provided: "My note: {note}\n\n"}Contents:

```{lang_from_extension}
{file contents verbatim}
```

Action: {verb}
```

**Path is relative to AgentCanvas root.** Examples:
- `Inbox/test.md`
- `Projects/AGRC/spec-v3.md`
- `Inbox/captures/2026-05-19-research-notes.md`

**`lang_from_extension`:** `markdown` for `.md`/`.markdown`, `html` for `.html`/`.htm`, raw fenced block otherwise.

**No "Path:" / "Project:" / "Persona inferred:" headers.** Drop them. The agent doesn't need them; the prose intro is enough.

**Note formatting:** If empty, omit the "My note:" line entirely.

### Fix 2 — Send popover (action verb picker + note input)

When user triggers Send (button click or ⌘⏎), open a small floating popover anchored near the action button. Popover contents:

```
┌──────────────────────────────────────┐
│ Send to {default-agent-label}        │
├──────────────────────────────────────┤
│ Action:                              │
│   ● Review     ○ Revise              │
│   ○ Expand     ○ Critique            │
│   ○ Summarize  ○ Respond to          │
│   ○ Custom: [______________]         │
│                                      │
│ Note (optional):                     │
│   [_______________________________]  │
│                                      │
│  [Cancel]              [Send ⏎]     │
└──────────────────────────────────────┘
```

**Keyboard wiring:**
- Tab cycles between action radios → custom input → note input → Send
- Arrow keys navigate action radios
- Enter from anywhere = Send
- Esc = Cancel

**Default action = Review** (most common).

**State:** action verb selection persists across sessions in SQLite (`settings` table, key=`default_action_verb`). Last-used verb is the next default.

**Custom verb:** typed string used as the action verb literally.

### Fix 3 — Dynamic button label + agent picker

**Button label logic** (in viewer toolbar + command palette):

| Condition | Label |
|-----------|-------|
| No agent sessions declared in panel | `Send to Agent` (button opens add-session prompt first) |
| One session declared | `Send to {persona·backbone}` (e.g. `Send to cto·claude`) |
| Multiple sessions, default set for this project/context | `Send to {default-persona·backbone}` |
| Multiple sessions, no default | `Send to Agent` (Send popover shows agent picker as first field) |

**Default agent for project:** stored in SQLite (`projects` table, `default_agent_session_id` FK). Set via right-click on agent card in panel → "Set as default for {current project}" OR via command palette "Switch Agent Default..."

**Keybindings:**
- `⌘⏎` = Send to default agent (or open Send popover with default selected)
- `⇧⌘⏎` = Open Send popover with agent picker as first field

### Fix 4 — Drag-and-drop file operations

**4a. Finder → AgentCanvas window (drop into Inbox area or sidebar Inbox row):**
- Accept dropped files of any type
- Copy to `~/iCloud/AgentCanvas/Inbox/` (preserve filename; on collision, append `-{n}` before extension)
- Show "+ {filename}" toast
- Refresh inbox list
- New file gets the "just arrived" highlight (blue dot + brief pulse)

**4b. Inbox row → Project folder in sidebar:**
- Visual drop target highlight on hover (use `--drop-target-bg` and `--drop-target-border` from visual-system.md)
- On drop: move file from `Inbox/` to `Projects/{name}/` using `std::fs::rename` (atomic on same filesystem)
- Update `files` table: change `path`, preserve `last_seen_hash`, `size`, `pinned`, `last_read_at`
- Refresh both inbox + project file list
- Toast: "Moved {filename} → {project}"

**4c. Inbox row → Archive (sidebar):**
- Same pattern as 4b but target is `Archive/`
- Mark `archived=1` in files table

**4d. Conflict handling:**
- If dragged file collides with existing filename at target, prompt: "Replace {filename}?" with Replace / Keep Both (append `-{n}`) / Cancel options

**Tauri API:** Use Tauri 2 file-drop event listener at window level. Use HTML5 drag-and-drop API for in-app drag (inbox row → sidebar target).

### Fix 5 — Right-click context menu on file rows

Right-click any inbox or project file row → context menu with:
- Open (default — also from double-click)
- Toggle Pin (⌘P)
- File to Project → submenu of project names
- Archive (⌘⌫)
- Send to Agent... (⌘⏎)
- Reveal in Finder
- Copy Path (relative)
- Delete... (with confirmation; permanently removes file from disk and from `files` table)

Implementation: HTML `oncontextmenu` handler on file row component; render custom menu component (not native macOS menu — keeps cross-platform consistency).

### Fix 6 — Open files dialog as alternative to drag-drop

Add a `+` button or "Open file..." menu action that opens a native file picker. Selected file gets copied to Inbox (same flow as 4a).

This is the keyboard-accessible alternative for users who don't want to drag.

---

## Build Sequence

Atomic commits per slice. Conventional commits format.

### Slice 1 — Restructured payload + dynamic button label (~15 min)

1. Update Rust handoff payload formatter to new format (Fix 1).
2. Update UI button label logic (Fix 3) — read current sessions + current project default from SQLite, render dynamic label.
3. Update relative path computation: strip AgentCanvas root prefix.
4. Verify clipboard output matches new format with a unit test.
5. Commit: `feat(handoff): restructured payload + dynamic agent label`

### Slice 2 — Send popover (action verb + note input) (~25 min)

1. Build `<SendPopover />` React component with radio group + custom input + note input + Cancel/Send.
2. Wire keyboard: Tab cycles, arrows on radios, Enter sends, Esc cancels.
3. Persist last-used action verb in SQLite `settings` table.
4. Trigger popover from Send button click + ⌘⏎.
5. Integrate with existing handoff payload builder (passes selected verb + note).
6. Commit: `feat(ui): send popover with action verb picker + note input`

### Slice 3 — Default-agent-per-project (~20 min)

1. Add `default_agent_session_id` column to `projects` table (migration).
2. Right-click context menu on agent panel cards: "Set as default for {project}".
3. Command palette action: "Switch Agent Default..."
4. Send popover respects per-project default.
5. `⇧⌘⏎` opens popover with agent picker visible.
6. Commit: `feat(agents): per-project default agent + picker shortcut`

### Slice 4 — Drag-and-drop file operations (~30 min)

1. Tauri file-drop listener at window level (Fix 4a).
2. HTML5 drag-drop on inbox rows (Fix 4b/4c).
3. Drop target highlight CSS using visual-system tokens.
4. Conflict prompt (Fix 4d).
5. State.db updates on move; preserve artifact identity per A10.
6. Toast feedback for all operations.
7. Commit: `feat(dnd): drag-and-drop file moves (finder→inbox, inbox→project, inbox→archive)`

### Slice 5 — Right-click context menu + open dialog (~15 min)

1. `<FileContextMenu />` component, right-click anchored.
2. Wire all menu items (most already have command handlers; reuse).
3. "Open file..." menu action with native file picker → copies to Inbox.
4. Commit: `feat(ui): file row context menu + open-file dialog`

### Slice 6 — Smoke + release (~10 min)

1. End-to-end test:
   - Drag a file from Finder → appears in Inbox with "just arrived" highlight
   - Right-click → File to Project → AGRC → file moves
   - Open the file → click Send (or ⌘⏎) → popover opens → select "Revise" → type note → Send → clipboard contains new payload format
   - Verify payload has no "Path:" header, has relative path, has fenced code block, has action verb
2. Update README.md with new behaviors (drag-drop, context menu, send popover).
3. Bump tauri.conf.json + crate version to `0.1.1`.
4. Tag `v0.1.1`.
5. Commit: `chore(release): v0.1.1 — drag-drop + restructured handoff + context menu`

---

## Acceptance Criteria

1. Drag a file from Finder onto the AgentCanvas window → file appears in Inbox within 1 second.
2. Drag an Inbox row onto a Project folder in sidebar → file moves; state preserved.
3. Right-click any file row → context menu appears with all 8 items.
4. Click Send (or ⌘⏎) → popover opens with action verb picker.
5. Select "Revise", type a note, press Enter → clipboard contains payload with relative path, fenced code block, "Action: Revise", and the note in "My note: ..." line.
6. Button label updates dynamically: empty agent panel = "Send to Agent"; one session = "Send to cto·claude" etc.
7. Set agent as default for project → next ⌘⏎ uses that agent.
8. Esc in popover dismisses without sending.
9. Custom verb input accepted as literal action verb.
10. v0.1.0 acceptance criteria 1-13 still pass.

---

## Out of Scope for v0.1.1

- Live MCP server (still v0.2)
- Comments (still v0.2)
- Annotation toolbar (still v0.2)
- PNG/JSON/TXT viewers (still v0.2+)
- Action-verb templates with preset prompts (e.g., "Critique with specific frame...") — v0.2
- Multi-file Send (send 3 inbox items at once) — v0.2

---

## Constraints

- Atomic commits per slice with conventional commit format
- Implemented-by Codex, Planned-by Claude, Co-Authored-By Codex attribution on every commit
- Direct push to main (no PRs)
- Visual system: pure additions to `visual-system.md` only if absolutely needed; reuse existing tokens
- All Acceptance Criteria must pass before declaring v0.1.1 done

---

## Where to Find Things

| Need | Location |
|------|----------|
| Patch spec (this doc) | `docs/PATCH-SPEC-v0.1.1.md` |
| v0 build context | `BUILD-SPEC-v0.md` |
| Current handoff impl | search `crates/agent-canvas-app/src/` for clipboard/handoff |
| UI components | `ui/src/` |
| State DB schema | search for `CREATE TABLE` in Rust source |
| Visual tokens | `prototypes/visual-system.md` |
