# Codex Slice 5e-g Report — 2026-05-20

## Files Modified

- `crates/vellum-core/src/sidecar/mod.rs`
- `crates/agent-canvas-app/src/main.rs`
- `ui/src/App.tsx`
- `ui/src/components/SourceView.tsx`
- `ui/src/pm/inlinePayload.ts`
- `ui/src/pm/schema.ts`
- `ui/src/types/blocks.ts`
- `ui/src/types/generated/BaseSnapshot.ts`
- `ui/src/types/generated/IdentityMap.ts`
- `ui/src/styles.css`
- `prototypes/visual-system.md`
- `status.md`

## ProseMirror Editing Decision

Fallback to source editor.

Reason: `legacy/vellum-spec-v0.3.md` specifies a ProseMirror-owned block patch layer with stable block IDs, branch resolution, block splits/merges, and serializer fallback rules. The current AgentCanvas UI/Rust contract does not yet expose that round-trip adapter. Implementing full rendered editing here would require inventing the missing PM-to-BlockPatch layer inside this slice. Instead, Markdown edit mode now uses CodeMirror source editing and shows the required hint: "Rendered-view editing lands in v0.3 — using source editor".

## 3-Way Merge Decision

Full 3-column merge dialog.

Reason: sidecar plumbing was small and backward-compatible. `IdentityMap` now accepts optional `base_snapshot: { hash, source }`; missing snapshots still load. `write_document` updates the sidecar snapshot after each successful save. On conflict, the UI loads the sidecar snapshot and the current on-disk document, then displays "Your draft", "Common ancestor", and "On disk now".

## New Components

- `AnnotationToolbar` in `ui/src/App.tsx`
- `ConflictMergeDialog` in `ui/src/App.tsx`

`SourceView` now exposes an imperative `applyFormat()` handle for source-backed annotation commands and reports selection bounds for toolbar placement. The ProseMirror schema also has a lightweight `revision` mark so source-authored `<mark data-revision="true">...</mark>` spans render as yellow highlights.

## New Deps

None.

## Verification Results

- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && pnpm install --no-frozen-lockfile'` failed because `ui/pnpm-workspace.yaml` has no `packages` field.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && CI=true pnpm --ignore-workspace install --no-frozen-lockfile'` passed.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && node node_modules/.pnpm/typescript@5.9.3/node_modules/typescript/bin/tsc --noEmit'` passed.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && pnpm --ignore-workspace build'` passed. Vite reported the existing large-chunk warning.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/crates/agent-canvas-app && cargo check'` passed.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -5'` passed: 6 tests.
