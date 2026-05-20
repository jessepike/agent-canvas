# v0.3 Slice 3 — File-level Comments + Unified Comments UI

You are implementing Slice 3 of AgentCanvas v0.3. Read `docs/BUILD-SPEC-v0.3.md`, especially Slice 3 (lines 164-173), and review Slice 2's discriminated-union `CommentAnchor` work in commit `93cdf96` (`crates/vellum-core/src/sidecar/mod.rs`, `ui/src/types/blocks.ts`, `ui/src/App.tsx`).

## What "done" looks like

1. **PNG, PDF, JSON, TXT viewers each have an "Add comment about this file" button.** Clicking opens the comment dialog. Submitting writes a comment with anchor `{ kind: "file_level" }` into the sidecar.
2. **Markdown and HTML viewers also get the same button** so users can attach file-level commentary alongside text selections. (The spec says PNG/PDF/JSON/TXT explicitly but symmetry is right — and it's a one-line addition to those viewers.)
3. **Comments panel groups by anchor kind.** Text selections (anchor `text_selection` and `html_selection`) at the top under a "Selections" heading; file-level comments at the bottom under a "About this file" heading. Section headings hidden when a section is empty.
4. **Clicking a file-level comment** does NOT trigger scroll-to-selection. Hover highlighting still works for visual feedback, but no jump.
5. **Legacy sidecars unchanged.** Existing markdown comments continue to deserialize and render correctly.

## Implementation plan

### 1. Anchor schema — add the `file_level` variant

Extend the Rust enum and the TS type:

```rust
// crates/vellum-core/src/sidecar/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CommentAnchor {
    TextSelection(TextCommentAnchor),
    HtmlSelection(HtmlCommentAnchor),
    FileLevel(FileLevelAnchor),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileLevelAnchor {
    pub kind: FileLevelKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FileLevelKind {
    #[serde(rename = "file_level")]
    FileLevel,
}
```

Pick whatever simplest serde shape makes `{ "kind": "file_level" }` round-trip cleanly. The untagged enum order matters — `FileLevel` must come AFTER `HtmlSelection` (which has more required fields) so deserialization doesn't accidentally collapse to it. Test both legacy and new shapes.

TS:

```ts
type CommentAnchor =
  | { kind?: "text_selection"; block_id: string | null; start_offset: number; end_offset: number }
  | { kind: "html_selection"; start_offset: number; end_offset: number; snapshot_text: string }
  | { kind: "file_level" };
```

Add Rust unit test: `file_level_comment_anchor_round_trips`.

### 2. UI — "Add comment about this file" button

Each viewer surface gets a small affordance. The cleanest placement: in the existing toolbar / breadcrumb area above the rendered content, NOT inside the viewer body (don't pollute PDFs / images). Re-use an existing toolbar location if there's a natural one; otherwise add a thin "viewer-toolbar" row above the content area for the affordance.

- PNG viewer (line ~1961)
- PDF viewer (line ~1968)
- JSON tree (line ~1945) — show button regardless of jsonViewMode
- TXT viewer (line ~1949)
- Markdown rendered view (line ~1944, the `RenderedView` panel) — show alongside the existing comments path
- HTML iframe view (line ~1955) — show in the host toolbar, not inside the iframe

Button label: **"Add comment about this file"**.
Button placement: top-right of the viewer panel works. Use existing toolbar/header styling, not a primary button.
Behavior: click opens `CommentDialog`. On submit, write a comment with anchor `{ kind: "file_level" }`.

The cleanest implementation: add a single `FileLevelCommentButton` component used in every viewer. New state: `fileLevelDialogOpen: boolean`. New handler: `submitFileLevelComment(body)` that:
- builds `CommentAnchor` of `kind: "file_level"`
- pushes into `comments`
- writes sidecar via `updateSidecarComments`
- closes dialog

Re-use the existing `CommentDialog` component. Possibly extend it to accept an optional title prop ("Comment on selection" vs "Comment on file").

### 3. Comments panel grouping

Update `CommentsPanel` in `ui/src/App.tsx`:

```tsx
function CommentsPanel({ comments, ... }) {
  const open = comments.filter(c => !c.resolved);
  const selections = open.filter(c => c.anchor.kind !== "file_level");
  const fileLevel = open.filter(c => c.anchor.kind === "file_level");
  
  return (
    <aside className="comments-panel">
      <div className="agent-panel-header">
        <span>Comments</span>
        <span className="count">{open.length}</span>
      </div>
      <div className="comments-list">
        {selections.length > 0 && (
          <>
            <div className="comments-section-label">Selections</div>
            {selections.map(renderCard)}
          </>
        )}
        {fileLevel.length > 0 && (
          <>
            <div className="comments-section-label">About this file</div>
            {fileLevel.map(renderCard)}
          </>
        )}
        {open.length === 0 && <div className="empty-list">No comments</div>}
      </div>
    </aside>
  );
}
```

CSS: add `.comments-section-label` styled like `.context-menu-label` (small caps, tertiary text). Reuse `--text-tertiary`; do NOT introduce a new raw hex.

### 4. Click behavior for file-level comments

When a file-level comment is clicked, skip the scroll-to-selection path. In the existing `onSelect(comment)` callback, branch on `anchor.kind`:

```tsx
if (comment.anchor.kind === "file_level") {
  // No scroll. Just keep card "active" for visual feedback.
  setHoveredCommentId(comment.id);
  return;
}
// existing scroll-to logic
```

For HTML-selection comments, the existing Slice 2 `agentcanvas:scroll_to` path remains.

## Files you will touch

- `crates/vellum-core/src/sidecar/mod.rs` — `FileLevel` variant + tests
- `crates/agent-canvas-app/src/main.rs` — new round-trip test
- `ui/src/App.tsx` — file-level button in 6 viewers, new handler, CommentsPanel grouping, onSelect branch
- `ui/src/types/blocks.ts` — CommentAnchor union extension
- `ui/src/types/generated/CommentAnchor.ts` — regenerate (or manual update if ts-rs is still warning-noisy from Slice 2)
- `ui/src/styles.css` — `.comments-section-label`, viewer-toolbar styling if you add one
- `status.md` — Slice 3 session log entry
- `BACKLOG.md` — close Slice 3, surface any spinoffs

## Hard constraints

- Do not modify `intent.md`, `legacy/vellum-spec-v0.3.md`, `BUILD-SPEC-v0.3.md`.
- A15: no raw hex outside `:root`. New `.comments-section-label` uses CSS vars.
- A22: do not touch iframe sandbox flags.
- Legacy markdown sidecars must continue to load. Add a regression test that deserializes a Slice 1-era comment file.
- File-level button placement must not break PDF / PNG rendering — put it OUTSIDE the viewer body, in the toolbar.
- Do not implement MCP tools (Slice 4+).
- Do not change Send-back behavior (Slice 6).

## Verification you must run

```bash
# Rust
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -20
cd crates/vellum-core && cargo test 2>&1 | tail -15

# Frontend
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5

# Invariant audits
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l   # must be 0
awk 'BEGIN{inroot=0} /^:root[[:space:]]*\{/ {inroot=1} inroot && /^}/ {inroot=0; next} !inroot && /#[0-9A-Fa-f]{3,8}/ {print FILENAME ":" FNR ":" $0}' ui/src/*.css   # must be empty
```

All pass before commit.

## Report format

Write `docs/active/codex-slice3-v0.3-report-2026-05-20.md` with:
1. Files modified
2. CommentAnchor union final shape (Rust + TS)
3. Where the file-level button lives in each viewer
4. CommentsPanel grouping screenshot equivalent (paste the section-label JSX)
5. Tests added
6. Verification command output
7. Invariant audit
8. Known issues / gaps

Commit message:

```
feat(v0.3-slice3): file-level comments on all viewers, grouped comments panel
```

Single atomic commit.

## Out of scope (do NOT build)

- MCP tools
- Send-back wiring
- Per-comment resolve-to-disk action
- Threaded replies on comments
- Per-file comment counters in the file list
- Notifications when an agent adds a comment

If you notice an improvement adjacent to this slice, write it to `BACKLOG.md` with a `[v0.3-slice3-spinoff]` tag. Do not build it.
