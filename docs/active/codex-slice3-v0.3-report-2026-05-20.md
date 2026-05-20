# Codex Slice 3 v0.3 Report — 2026-05-20

## 1. Files modified

- `crates/vellum-core/src/sidecar/mod.rs`
- `crates/agent-canvas-app/src/main.rs`
- `ui/src/App.tsx`
- `ui/src/styles.css`
- `ui/src/types/blocks.ts`
- `ui/src/types/generated/CommentAnchor.ts`
- `ui/src/types/generated/Comment.ts`
- `ui/src/types/generated/IdentityMap.ts`
- `ui/src/types/generated/FileLevelAnchor.ts`
- `ui/src/types/generated/FileLevelKind.ts`
- `ui/src/types/generated/HtmlCommentAnchor.ts`
- `ui/src/types/generated/HtmlCommentAnchorKind.ts`
- `ui/src/types/generated/TextCommentAnchor.ts`
- `ui/src/types/generated/TextCommentAnchorKind.ts`
- `status.md`
- `BACKLOG.md`
- `docs/active/codex-slice3-v0.3-report-2026-05-20.md`

## 2. CommentAnchor union final shape

Rust:

```rust
#[serde(untagged)]
pub enum CommentAnchor {
    HtmlSelection(HtmlCommentAnchor),
    TextSelection(TextCommentAnchor),
    FileLevel(FileLevelAnchor),
}

pub struct TextCommentAnchor {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<TextCommentAnchorKind>,
    pub block_id: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
}

pub struct HtmlCommentAnchor {
    pub kind: HtmlCommentAnchorKind,
    pub start_offset: usize,
    pub end_offset: usize,
    pub snapshot_text: String,
}

pub struct FileLevelAnchor {
    pub kind: FileLevelKind,
}
```

TypeScript:

```ts
export const CommentAnchor = z.union([
  z.object({
    kind: z.literal("html_selection"),
    start_offset: z.number().int().nonnegative(),
    end_offset: z.number().int().nonnegative(),
    snapshot_text: z.string()
  }).strict(),
  z.object({
    kind: z.literal("text_selection").optional(),
    block_id: z.string().nullable(),
    start_offset: z.number().int().nonnegative(),
    end_offset: z.number().int().nonnegative()
  }).strict(),
  z.object({
    kind: z.literal("file_level")
  }).strict()
]);
```

Generated TS now resolves as:

```ts
export type CommentAnchor = HtmlCommentAnchor | TextCommentAnchor | FileLevelAnchor;
```

## 3. File-level button placement

- Markdown rendered viewer: top-right `viewer-toolbar` above `.rendered-panel`.
- HTML iframe viewer: top-right host `viewer-toolbar` above `.html-panel`; iframe sandbox flags unchanged.
- JSON tree viewer: top-right `viewer-toolbar` above `.json-panel`.
- JSON source viewer: top-right `viewer-toolbar` above `.source-panel`.
- TXT viewer: top-right `viewer-toolbar` above `.source-panel`.
- PNG viewer: top-right `viewer-toolbar` above `.image-panel`, outside `.image-frame`.
- PDF viewer: top-right `viewer-toolbar` above `.pdf-panel`, outside the `<object>` body.

## 4. CommentsPanel grouping screenshot equivalent

```tsx
{selectionComments.length > 0 ? (
  <>
    <div className="comments-section-label">Selections</div>
    {selectionComments.map(renderCard)}
  </>
) : null}
{fileLevelComments.length > 0 ? (
  <>
    <div className="comments-section-label">About this file</div>
    {fileLevelComments.map(renderCard)}
  </>
) : null}
```

## 5. Tests added

- `crates/vellum-core/src/sidecar/mod.rs`
  - `file_level_comment_anchor_round_trips`
  - `legacy_markdown_comment_anchor_still_deserializes`
- `crates/agent-canvas-app/src/main.rs`
  - `file_level_comment_anchor_round_trips`

Existing Slice 2 regression coverage for legacy markdown and HTML anchors remains in `crates/agent-canvas-app/src/main.rs`.

## 6. Verification command output

Per repo instructions, Rust verification was run in the OrbStack `dev` VM.

`cd crates/agent-canvas-app && cargo check -q`

```text
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.
```

`cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -20`

```text
running 11 tests
test tests::html_comment_anchor_round_trips_with_snapshot_text ... ok
test tests::file_level_comment_anchor_round_trips ... ok
test tests::legacy_comment_anchor_deserializes_as_text_selection ... ok
test tests::send_payload_uses_relative_path_fence_note_and_action ... ok
test tests::send_payload_omits_empty_note_and_defaults_action ... ok
test tests::test_path_safe_for_canvas_allow_deny_matrix ... ok
test tests::test_path_within_canvas_resolves_symlinks ... ok
test tests::test_path_within_canvas_shim_accepts_safe_path ... ok
test tests::migration_backfills_legacy_tags_idempotently ... ok
test tests::untrack_keeps_file_delete_from_disk_removes_file ... ok
test tests::test_identity_relink_skips_when_old_path_exists ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cd crates/vellum-core && cargo test 2>&1 | tail -15`

```text
test rejects_patch_with_validation_error ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Doc-tests vellum_core
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cd ui && ./node_modules/.bin/tsc --noEmit`

```text
passes with no output
```

`cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5`

```text
(!) Some chunks are larger than 500 kB after minification. Consider:
- Using dynamic import() to code-split the application
- Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 990ms
```

## 7. Invariant audit

`grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l`

```text
0
```

`awk 'BEGIN{inroot=0} /^:root[[:space:]]*\{/ {inroot=1} inroot && /^}/ {inroot=0; next} !inroot && /#[0-9A-Fa-f]{3,8}/ {print FILENAME ":" FNR ":" $0}' ui/src/*.css`

```text
no output
```

## 8. Known issues / gaps

- Host macOS runs of `cargo test --bin agent-canvas-app` still fail on the pre-existing temp-path and legacy tag-backfill tests tracked in `BACKLOG.md`.
- Host macOS runs of `crates/vellum-core cargo test` can still hit the pre-existing watcher timeout. The same required Rust verification passes in OrbStack `dev`.
- `ts-rs` still emits the existing warning for `#[serde(skip_serializing_if = "Option::is_none")]`; `[v0.3-slice2-spinoff]` already tracks replacing the warning-prone generated-type strategy.
- Browser plugin verification could not be run because the Node REPL browser execution tool was not available after tool discovery; build/type/invariant verification passed.
