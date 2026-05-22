---
project: agent-canvas
spec: v0.4
status: draft
created: 2026-05-21
author: cto
supersedes: none
---

# AgentCanvas — Build Spec v0.4: New File + Default Opener + Provenance Inbox

## Goal

A connected set of changes that make AgentCanvas the daily-driver artifact surface:

1. **Relocate the canvas root** to a single, visible, iCloud-synced location.
2. **Provenance-segregated left pane** — Inbox is *agent-placed only*; user-created
   files get their own section; externally-opened files are ephemeral Recents.
3. **Unread state** — agent-placed files are visually "new" until opened (email-style).
4. **Create new files** in-app (Cmd-N).
5. **Be the default macOS opener** for `.md`, `.html`, `.txt`.

All serve intent.md: the Inbox-as-agent-artifact-stream metaphor, sub-second open,
source faithfulness, and the review-and-roundtrip loop. Segregating user files and
adding unread state *strengthens* the Inbox-as-stream identity rather than diluting it.

## Decisions (locked 2026-05-21, owner: Jesse)

| # | Decision | Choice |
|---|----------|--------|
| D1 | External files (opened from outside the canvas root) | **Ephemeral** — full read/edit/save-in-place, NOT tracked. Surfaced in a **Recents** list. |
| D2 | File types to register as default | `.md`, `.markdown`, `.html`, `.htm`, `.txt`. **Not** `.json`. |
| D3 | New-file behavior | Lands in **`MyFiles/`** folder (left-pane section labeled **"Drafts"**), as `.md`, name prompt, opens in **edit mode**. Cmd-N. |
| D4 | Set-as-default mechanism | Tauri config registration + `scripts/set-default-handler.sh` (`duti`) + manual Finder instructions. No programmatic Launch Services call. |
| D5 | **Canvas root location** | **Single root, relocated to `~/Documents/AgentCanvas/`** with subfolders `Inbox/`, `MyFiles/`, `Projects/`, `Archive/`. iCloud-synced (this Mac has Desktop & Documents → iCloud ON). Requires a one-time migration (Slice 0). Supersedes the old buried `…/com~apple~CloudDocs/AgentCanvas/` path. |
| D6 | Left-pane provenance | **Inbox** = agent-placed only (lands in `Inbox/`). **Drafts** = user-created (`MyFiles/`). **Recents** = ephemeral external opens. Provenance is decided **by folder location**, not an inferred column. |
| D7 | Unread / "new" state | Email-style: agent-placed files are **unread until opened** — row dot/bold + an **unread count on the Inbox header**; clears on first open; persists across restarts (`seen_at`). The existing momentary arrival pulse is retained. |

## Intent check

- `~/Documents/AgentCanvas/` is iCloud-synced → still satisfies intent's *"iCloud Drive
  is the sync substrate."* intent.md names no specific path, so **intent.md is not
  violated and is not edited** (it is locked/sacred).
- The CLAUDE.md non-negotiable invariant *does* name the old path → it **is rewritten as
  part of Slice 0**, atomically with the actual file move, so docs never lead reality.

## Non-Goals (v0.4)

- JSON / PNG / PDF default association.
- Programmatic "make me default" button (Launch Services via objc).
- Positional comments on PNG/PDF (separate v0.4 candidate, not this slice).
- New-file location/extension picker (D3 is fixed).
- iOS file associations.

## Architecture Notes

Substrate that already exists and carries this work:
- **Flavor 2 tag model** tracks files by absolute path → relocation and external-file
  opening need no new path machinery, only path *rewrites*.
- **Slice 6 watcher fix** watches a set of parent dirs with ref-counted unwatch → extends
  directly to (a) a multi-subfolder root and (b) transient watches on ephemeral files.
- **Atomic save + `base_hash` guard** works on any absolute path unchanged.
- **Sidecar comment auto-migration** (source-hash matching + cache-wide scan on miss)
  helps comments re-link after the move, but `files.path` must still be rewritten.

### Provenance resolution (folder-driven)

| Folder | Section | Tracked? | Unread-eligible? |
|--------|---------|----------|------------------|
| `~/Documents/AgentCanvas/Inbox/` | Inbox | yes | **yes** |
| `~/Documents/AgentCanvas/MyFiles/` | Drafts | yes | no |
| `~/Documents/AgentCanvas/Projects/{p}/` | Projects | yes | no (v0.4) |
| `~/Documents/AgentCanvas/Archive/` | Archive | yes | no |
| anywhere else on disk | Recents | **no (ephemeral)** | no |

### Tracked vs ephemeral resolution rule (open-by-path)

1. Path under the canvas root → tracked open, sectioned by the table above.
2. Path already has a `files` row → tracked open.
3. Otherwise → ephemeral open (editable, atomic-saved, transient watch, upsert `recents`,
   no `files` row).

### Tauri open-event timing (known gotcha)

`RunEvent::Opened { urls }` fires at startup before the webview attaches listeners.
Buffer URLs in app state; frontend pulls via `take_pending_opens()` on mount and also
listens for an `open-external` event for the warm path. Use
`tauri-plugin-single-instance` so a second Finder open routes to the running window.

## Slices

### Slice 0 — Canvas root relocation + cleanup (foundational, do first)

**Reality check (verified 2026-05-21 against live `state.db` + filesystem):**
- Code's current `canvas_root` is already `~/AgentCanvas` (`main.rs:1388`), NOT iCloud.
  The CLAUDE.md "iCloud" invariant is **stale vs code** — the pivot already happened.
- `legacy_icloud_canvas_root()` (`main.rs:1556`) = buried `com~apple~CloudDocs/AgentCanvas`,
  used only for tag backfill.
- Tracked files in DB: **only 2, both external** (`~/Downloads/…jpg`, repo
  `docs/user-guide.md`) — neither under any canvas root. **No canvas-root data to rewrite.**
- `~/AgentCanvas` = near-empty real dir (`Inbox/captures`, `Projects/Default`, `Archive`).
- iCloud `AgentCanvas` = 4 untracked legacy files (`jenna-*`, `index.html`, `test.md`).
- One stale `session_attachments` row → `…/com~apple~CloudDocs/ethan/prototypes/DECISIONS.md`
  (unrelated test pointer).

**Steps:**
1. *(orchestrator, pre-flight)* Quit the running app; WAL-checkpoint; timestamped
   `state.db` backup. (Done before handoff.)
2. Change code `canvas_root` → `home.join("Documents").join("AgentCanvas")`; add `MyFiles`
   to `ensure()` alongside Inbox/Projects/Archive. Centralize subfolder names.
3. Create `~/Documents/AgentCanvas/{Inbox,MyFiles,Projects,Archive}`.
4. **Preserve, don't delete:** move the 4 iCloud legacy files into the new root
   (Inbox→Inbox, Archive→Archive); move any `~/AgentCanvas` content likewise.
5. Extend legacy backfill to recognize **both** old roots (iCloud + `~/AgentCanvas`) so any
   straggler tracked path re-tags.
6. DB: delete the stale `ethan` `session_attachments` row; keep the 2 external tracked files.
7. Remove the old `~/AgentCanvas` dir and the iCloud `AgentCanvas` dir once emptied.
8. Docs: rewrite the **project `CLAUDE.md` invariant**; update `AGENTS.md`, `README.md`,
   `docs/user-guide.md`, `BUILD-SPEC-v0.md`. **Do NOT edit `intent.md`** (locked; not
   violated — `~/Documents` is still iCloud-synced). **Do NOT edit historical
   `docs/active/*` reports.**
9. Build + test in OrbStack; add/adjust a unit test asserting `resolve()` → Documents path.
- **Accept:** rebuilt app boots against `~/Documents/AgentCanvas`; new dirs incl `MyFiles`
  exist; the 2 external tracked files still listed; both old roots gone; tests green;
  `state.db` backup exists.

### Slice 1 — New file action → Drafts
- Command `create_my_file(name) -> path`: sanitize, force `.md`, atomic-write empty file
  into `MyFiles/`, collision suffix via existing strategy, track it (Drafts section),
  return path.
- UI: **"Drafts"** left-pane section listing `MyFiles/`. New action in toolbar + palette +
  **Cmd-N** → name prompt (existing focus-trapped dialog, no `window.prompt`) → open in
  **edit mode**.
- **Accept:** Cmd-N → name → file in `MyFiles/`, appears under Drafts (never Inbox), opens
  editable; empty/duplicate names handled.

### Slice 2 — Provenance sections + unread state
- Left pane renders **Inbox** (agent-placed, from `Inbox/`) and **Drafts** (`MyFiles/`) as
  distinct sections; Recents added in Slice 5.
- `files.seen_at INTEGER NULL` migration (idempotent). New Inbox arrivals (watcher/MCP)
  default `seen_at = NULL` → **unread**.
- UI: unread row treatment (dot/bold) + **unread count badge on the Inbox header**.
- First open of an unread file sets `seen_at = now` → clears its unread mark and decrements
  the count. Retain the existing arrival pulse for files that land while you're looking.
- **Accept:** an agent dropping a file into `Inbox/` shows it unread with the count
  incremented; opening it clears the mark and persists across restart; `MyFiles/` files are
  never marked unread.

### Slice 3 — File association config + single instance
- `bundle.fileAssociations` in `tauri.conf.json` for D2 extensions → standard UTIs
  (`net.daringfireball.markdown`/`public.markdown`, `public.html`, `public.plain-text`),
  `rank: "Alternate"`.
- Add `tauri-plugin-single-instance`; second-instance forwards argv/URLs to the running app.
- **Accept:** built `.app` appears under Finder → Open With for all D2 types;
  `CFBundleDocumentTypes` present in the bundle Info.plist.

### Slice 4 — Open-event handling + startup buffering
- `pending_opens: Mutex<Vec<PathBuf>>` in app state; `RunEvent::Opened` pushes paths and
  emits `open-external` when warm; `take_pending_opens()` command for cold-launch mount.
- Frontend routes each path through `open_path` (Slice 5); window raises/focuses on open.
- **Accept:** cold double-click opens the file; a second open while running routes to the
  same window.

### Slice 5 — Ephemeral open model + Recents
- `recents` table (idempotent): `path PK, last_opened, title`, capped (~50), pruned on insert.
- `open_path(path) -> OpenResult{mode, ...}` implementing the resolution rule.
- Ephemeral open: viewer/editor by absolute path, no `files` row; arm transient parent-dir
  watch; release on close; upsert `recents`. Save uses atomic+hash guard with no tracking
  side-effects.
- UI: **Recents** section reading `recents`, visually distinct from Inbox/Drafts; select
  re-opens via `open_path`.
- **Accept:** opening `~/Downloads/x.md` is editable, saves in place, not in Inbox/Drafts,
  appears in Recents; a file under the canvas root resolves to its tracked identity.

### Slice 6 — Set-default helper + smoke + release
- `scripts/set-default-handler.sh` (`duti` for D2 UTIs; prints manual Finder steps if absent).
- `docs/user-guide.md`: "New file", "Make AgentCanvas your default editor", "Where your
  files live" (the new root).
- Smoke: A22 (0), A15 (0), `cargo test`, `tsc`, `vite build`, MCP build. Bump 0.3.0 → 0.4.0
  across the four version files. Tag.
- **Accept:** gates green; `duti -x md` reports AgentCanvas.

## Verify-first item (before Slice 3 lands)

Confirm Tauri 2's `bundle.fileAssociations` emits a correct `CFBundleDocumentTypes` for
these extensions in the installed `.app` (`plutil -p …/Info.plist`). Known Tauri issue
around `LSHandlerRank` on macOS — if the generated mapping is wrong/missing rank, fall back
to an `Info.plist` partial merge.

### Slice 7 — Persistent agent messages + Agent Center (UX wave)

**Why:** The agent→user direction had working transport but no real UX. `notify_user`
was a transient 4s toast — no persistence, no acknowledgement, not actionable. The
right-hand agent pane was a passive roster with no job. Owner decisions (2026-05-22):
messages are **sticky until acknowledged, no history archive**; the right pane becomes an
**Agent Center** = presence (top) + unacknowledged-message queue (bottom).

**Reconciled model:** an agent message persists until the user **acknowledges** it, then it
is deleted (no archive). Unacknowledged messages survive an app restart (persisted in
`state.db`). The on-arrival toast and the Agent Center "Messages" section render the *same*
unacknowledged set; acknowledging removes it from both.

- **Backend — `agent_messages` table** (sessions.rs migration, mirror `user_messages`):
  `id TEXT PK, session_id TEXT NOT NULL, severity TEXT NOT NULL, message TEXT NOT NULL,
  action_artifact_path TEXT, action_label TEXT, created_at INTEGER NOT NULL,
  acknowledged_at INTEGER` + index on `acknowledged_at`. Register in
  `initialize_state_db`.
- **Backend — `notify_user` inserts a row** (in addition to emitting), so the message is
  durable. Commands: `list_agent_messages()` → unacknowledged only (ordered newest-first);
  `acknowledge_agent_message(id)` sets `acknowledged_at`. Emit
  `agentcanvas://messages-changed` **post-lock** (lock discipline — insert/ack under the db
  guard, emit after it drops). Prefer delete-on-ack to keep the table small (still "no
  history" to the user).
- **Frontend — sticky notification:** replace the auto-dismiss toast with a sticky
  banner/stack that stays until **Acknowledge**. On mount, hydrate from
  `list_agent_messages()`; refresh on `messages-changed` and on the live `notify-user`
  event. Each item: severity styling, source agent (persona/agent), message, **Open
  artifact** (if `action_artifact_path`, routes through `open_path`), **Acknowledge** →
  `acknowledge_agent_message(id)`.
- **Frontend — Agent Center (right pane):** two sections. **Presence**: existing live-agent
  roster + attached artifact + Disconnect (already cleaned via sessions-changed). **Messages**:
  the same unacknowledged queue as the sticky stack, each with Open-artifact + Acknowledge.
- **Send-button default fix:** the default Send target must name a real live agent
  (persona/agent label), default to the most-recently-active live MCP session, and fall
  back to a plain "Send" (clipboard) label when no live agent exists — never
  "default·unknown". Drop the junk fallback persona/session from the visible label.
- **Accept:** an agent `notify_user` produces a message that stays until acknowledged,
  appears in both the sticky stack and the Agent Center Messages list, opens its artifact on
  click, and disappears from both on Acknowledge; survives an app restart while
  unacknowledged; the Send button never shows "default·unknown". Gates: `cargo test`,
  `tsc`, `vite build`, A22=0, A15=0.

## Open questions (capture, don't build)

- One-click **"Track this"** promotion for ephemeral files (Recents → Inbox/Drafts)?
- Recents surface: sidebar section vs palette-only.
- Should agent-placed files into a **Project** folder also be unread-eligible (D7 limits
  unread to Inbox for v0.4)?
- Old iCloud root after migration: leave a tombstone README, or fully remove once verified?
