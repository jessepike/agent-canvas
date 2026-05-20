# v0.3 Slice 2 — Interactive HTML + Selection Bridge + HTML Comments

You are implementing Slice 2 of AgentCanvas v0.3. Read `docs/BUILD-SPEC-v0.3.md` first, especially Slice 2 (lines 147-162), invariants A22 and A8, and decisions D5 and D14.

## What "done" looks like

Three outcomes, all visually verifiable in the running app:

1. **Interactive HTML works.** An HTML file with `<button onclick="doSomething()">Click</button>` actually fires its handler when clicked. Forms submit. `fetch()` to public APIs works. `console.log` / `console.error` from inside the iframe appear in the app's UI error banner.
2. **Selection in HTML opens the comment dialog.** Select text inside the rendered HTML, press ⌘⇧M, the AnnotationToolbar / comment dialog opens with the selected text. Submitting writes a comment with anchor `{ kind: "html_selection", start_offset, end_offset, snapshot_text }` into the sidecar.
3. **HTML files have comments that survive reloads.** Open the HTML file, comments panel shows them, click a comment, the iframe scrolls to and highlights the snapshot text.

## Locked architecture (do not deviate)

Per A22 / D5:

```jsx
<iframe
  sandbox="allow-scripts allow-forms allow-popups allow-downloads"
  srcDoc={enrichedHtml}
  ref={iframeRef}
/>
```

- **No `allow-same-origin`.** (Iframe in opaque origin.)
- **No `allow-modals`.** (`alert` / `confirm` / `prompt` silently no-op.)
- These flags are invariant — guard with a unit test or invariant audit that grep'd `allow-same-origin` and `allow-modals` returns zero hits in `ui/src/`.

## Implementation plan

### 1. Cargo + tauri.conf additions

- Add `tauri-plugin-persisted-scope` to `crates/agent-canvas-app/Cargo.toml` (under `dependencies`) and to workspace `[workspace.dependencies]` if appropriate. Version `"2"`.
- In `crates/agent-canvas-app/src/main.rs`, register the plugin in the Tauri builder: `.plugin(tauri_plugin_persisted_scope::init())`.
- In `crates/agent-canvas-app/tauri.conf.json`, under `"app"`, add (or extend) `"security"` with `"assetProtocol"` that allows `$HOME/**` and denies system paths matching the existing `path_safe_for_canvas` deny list. Use Tauri 2 schema. Reference: https://v2.tauri.app/reference/config/#assetprotocolconfig
- Add Tauri capability for the asset protocol if required by Tauri 2's permission model.

### 2. Host bootstrap injection

The iframe must include a host-injected `<script>` that:

- Listens for `selectionchange` and posts `{ type: "agentcanvas:selection", range: { startOffset, endOffset, text } }` to `window.parent` via `postMessage` whenever the user selects text inside the iframe. Offsets are character offsets into `document.body.innerText`. Debounce by 80ms.
- Wraps `console.error` and `console.warn`: posts `{ type: "agentcanvas:console", level: "error"|"warn", message: string }` to parent.
- Exposes `window.agentcanvas.sendBack({ note?, action_verb? })` on the iframe `window`. Implementation just posts `{ type: "agentcanvas:send_back", payload }` to parent. (Wiring the parent to actually trigger Send-back is deferred to Slice 6 — for now host should log receipt and console.info that the API works.)
- Provides `window.agentcanvas.scrollToSnapshot(text)`: scrolls the iframe to the first occurrence of `text` (TextWalker / Range API), wraps it in a `<mark>` with class `agentcanvas-comment-highlight`, removes after 1500ms. Used by host to scroll-to-comment.
- On `message` event from parent with `{ type: "agentcanvas:scroll_to", text }`, calls `scrollToSnapshot(text)`.

Pick the cleanest injection mechanism: prefix `<script>` injection into the HTML source string before passing to `srcDoc`, OR `iframe.onload` then `contentWindow.eval` (won't work without `allow-same-origin` — confirm). **The right answer is prefix injection** because `allow-same-origin` is forbidden by A22. Insert immediately after `<head>` if present, otherwise at the start of the document.

### 3. Host postMessage receiver

In `App.tsx`, install a `window.addEventListener("message", ...)` handler that:

- Validates `event.source === iframeRef.current?.contentWindow`. Drop messages from other sources.
- Validates `event.data?.type?.startsWith("agentcanvas:")`. Drop anything else.
- Routes:
  - `agentcanvas:selection` → calls `setAnnotationSelection({ kind: "html", startOffset, endOffset, text })`. Define the HTML annotation selection shape so existing `openCommentDialog` works on it.
  - `agentcanvas:console` (level=error) → surfaces in the existing error banner with prefix `[iframe]`.
  - `agentcanvas:send_back` → for Slice 2, log to host console and surface a toast "Send-back received (Slice 6 will wire this)". Do not implement send-back routing yet.

### 4. Comment anchor schema evolution

Current anchor (legacy):

```ts
{ block_id: string | null, start_offset: number, end_offset: number }
```

Migrate to a discriminated union. Add optional `kind` field. Legacy comments without `kind` are treated as `kind: "text_selection"`. New HTML comments use `kind: "html_selection"` with `snapshot_text`. (Slice 3 will add `kind: "file_level"` — leave room.)

```ts
type CommentAnchor =
  | { kind?: "text_selection"; block_id: string | null; start_offset: number; end_offset: number }
  | { kind: "html_selection"; start_offset: number; end_offset: number; snapshot_text: string };
```

Mirror the type in Rust (`crates/agent-canvas-app/src/main.rs`) using `#[serde(tag = "kind", rename_all = "snake_case")]` if the existing serde structure permits, or with an optional `kind` field. Existing sidecar files must continue to deserialize unchanged. Add a migration test for a legacy comment file.

### 5. AnnotationToolbar gating

Currently the toolbar shows only for `artifact?.kind === "md" && editMode`. Extend so HTML selections also surface a comment affordance:

- For HTML, `editMode` is irrelevant — comments work on rendered HTML directly.
- Add a path: when `annotationSelection.kind === "html"`, render the comment-only AnnotationToolbar (no formatting buttons) anchored to a fixed position in the iframe overlay, OR keep the existing keyboard shortcut ⌘⇧M as the only entry. Pick the simpler one — keyboard shortcut alone is acceptable for Slice 2.

### 6. Sidebar scroll-to-comment for HTML

In the existing "click comment to scroll" flow: if the artifact is HTML and the anchor `kind === "html_selection"`, post `{ type: "agentcanvas:scroll_to", text: snapshot_text }` to `iframeRef.current?.contentWindow` instead of running the markdown block-scroll path.

## Files you will touch

- `crates/agent-canvas-app/Cargo.toml` (and workspace root `Cargo.toml`) — add plugin
- `crates/agent-canvas-app/src/main.rs` — register plugin, extend CommentAnchor serde, add unit tests
- `crates/agent-canvas-app/tauri.conf.json` — assetProtocol config
- `crates/agent-canvas-app/capabilities/default.json` (or wherever capabilities live) — grant asset protocol if required
- `ui/src/App.tsx` — iframe sandbox flags, postMessage receiver, anchor union, comment scroll-to for HTML
- `ui/src/ipc.ts` — any new wrappers (probably none for Slice 2)
- New file: `ui/src/htmlBootstrap.ts` — exports `BOOTSTRAP_SCRIPT` constant (the script string to inject) and `injectBootstrap(html: string): string` helper. Keep the script as a template string in one place.
- `status.md` — add Slice 2 session log entry
- `lessons.md` — capture anything surprising

## Hard constraints

- A22 sandbox flags are immutable. Any code path that produces an iframe with different flags is a bug.
- A8: HTML scripts are enabled by default (no per-file toggle in v0.3). Trust is implicit — user trusts the agent producing the file.
- A15: zero raw hex outside `:root`. The `agentcanvas-comment-highlight` class must use a CSS variable, not a hardcoded yellow.
- A17: no `window.prompt` / `window.confirm` / `window.alert` in host code. (And those silently no-op inside the iframe anyway.)
- Do not modify `intent.md`, `legacy/vellum-spec-v0.3.md`, `BUILD-SPEC-v0.3.md`. Read-only references.
- Do not touch MCP code — Slice 4+ territory.
- Do not implement send-back routing — Slice 6 territory. For Slice 2, the `sendBack` API just surfaces a toast.

## Verification you must run yourself

Run from `/Users/jessepike/code/sandbox/agent-canvas/` in the dev VM (mount: `/mnt/mac/Users/jessepike/code/sandbox/agent-canvas/`):

```bash
# Rust
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -20

# Frontend
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5

# Invariant audits
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l   # must be 0
grep -rn 'sandbox="allow-scripts allow-forms allow-popups allow-downloads"' ui/src/ | wc -l   # must be ≥ 1
```

All checks must pass. Don't ship if cargo check fails or if the invariant audits return wrong counts.

## Report format

Write `docs/active/codex-slice2-v0.3-report-2026-05-20.md` with:

1. Files modified (list)
2. Migration / schema changes (CommentAnchor union shape, serde behavior)
3. Plugin / config additions (tauri-plugin-persisted-scope, assetProtocol JSON)
4. New host bootstrap script — paste the final content
5. Tests added
6. Verification results (paste actual output of the commands above)
7. Invariant audit (A22 flags, A15 raw hex, A17 no native dialogs)
8. Known issues / gaps for me to verify on host

Commit message:

```
feat(v0.3-slice2): interactive HTML viewer, postMessage bridge, comments-on-HTML
```

Single commit. Atomic.

## Out of scope for this slice (do NOT build)

- MCP server wiring of any kind
- Send-back routing to MCP sessions
- File-level comments on PNG/PDF/JSON/TXT (Slice 3)
- Persona-aware iframe behavior
- Multi-iframe coordination
- Any UI changes outside the HTML view path

If you notice an improvement adjacent to this slice, write it to `BACKLOG.md` with a `[v0.3-slice2-spinoff]` tag. Do not build it.
