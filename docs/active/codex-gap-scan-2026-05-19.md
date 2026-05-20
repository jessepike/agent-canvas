```md
1. Exposed document commands are not root-bound to AgentCanvas
   Where: crates/agent-canvas-app/src/main.rs:529, :546, :563, :577
   What's promised vs what exists: Spec says only files under `~/iCloud/AgentCanvas/` are shown/touched. `open_document`, `write_document`, `load_sidecar`, and `save_sidecar` accept any absolute regular file path.
   Severity: blocker
   Suggested fix scope: small (~30min)

2. Artifact identity by hash can merge same-content duplicates
   Where: crates/agent-canvas-app/src/main.rs:856
   What's promised vs what exists: Rename state should relink by path+content history. Current hash-only relink can transfer pinned/read/archive state between two separate files with identical content.
   Severity: friction
   Suggested fix scope: medium (~1hr)

3. `last_read_at` is schema-only and never updated
   Where: BUILD-SPEC-v0.md:84, crates/agent-canvas-app/src/main.rs:66, :806
   What's promised vs what exists: State should hold read/last-viewed timestamps. Field exists and hydrates, but no open path updates it.
   Severity: polish
   Suggested fix scope: small (~30min)

4. Persona registry colors are read but not applied dynamically
   Where: BUILD-SPEC-v0.md:120, crates/agent-canvas-app/src/main.rs:901, ui/src/styles.css:408
   What's promised vs what exists: pike-agents `color:` frontmatter is canonical. Rust reads/caches it, but UI ignores `Persona.color` and uses hard-coded badge CSS classes.
   Severity: friction
   Suggested fix scope: medium (~1hr)

5. Persona CSS tokens missing from visual-system source
   Where: BUILD-SPEC-v0.md:134, CLAUDE.md:28, prototypes/visual-system.md, ui/src/styles.css:44
   What's promised vs what exists: Persona tokens should be defined in `visual-system.md` before use. Live CSS defines `--persona-*`; visual-system does not.
   Severity: polish
   Suggested fix scope: trivial (<15min)

6. Live CSS still contains ad-hoc colors outside token system
   Where: CLAUDE.md:28, prototypes/visual-system.md:129, ui/src/styles.css:87, :145, :637
   What's promised vs what exists: Visual tokens are authoritative. CSS uses raw gradient colors, traffic-light colors, white text, overlay rgba, and extra neutrals.
   Severity: polish
   Suggested fix scope: medium (~1hr)

7. Project sidebar counts are hard-coded to zero
   Where: ui/src/App.tsx:894, :919
   What's promised vs what exists: Sidebar count affordances imply real counts. Project rows always render `0`.
   Severity: polish
   Suggested fix scope: small (~30min)

8. Command palette "Open Project" is not real project selection
   Where: BUILD-SPEC-v0.md:289, ui/src/App.tsx:718
   What's promised vs what exists: Palette should support typeahead across commands/files. `Open Project` always opens `projects[0]`.
   Severity: friction
   Suggested fix scope: small (~30min)

9. Send/default-agent conflict flows use blocking prompts
   Where: docs/PATCH-SPEC-v0.1.1.md:135, :183, ui/src/App.tsx:477, :1457
   What's promised vs what exists: Spec describes explicit Replace / Keep Both / Cancel and palette default switching. Implementation uses `window.prompt`.
   Severity: friction
   Suggested fix scope: medium (~1hr)

10. Popovers and overlays lack dialog semantics/focus containment
    Where: docs/PATCH-SPEC-v0.1.1.md:86, ui/src/App.tsx:1126, :1181, :1196, :1311
    What's promised vs what exists: Send UI is specified as keyboard-accessible. Popovers/menus have no dialog roles, aria-modal, focus trap, or focus restoration.
    Severity: friction
    Suggested fix scope: medium (~1hr)

11. Acceptance criteria lack UI/integration coverage
    Where: BUILD-SPEC-v0.md:326, docs/PATCH-SPEC-v0.1.1.md:223, status.md:69, ui/package.json:6
    What's promised vs what exists: Many criteria are “Pass by implementation.” Rust substrate tests exist, but no UI test script covers send popover, drag/drop, context menu, keyboard nav, palette, or focus rescan.
    Severity: friction
    Suggested fix scope: large (>1hr)

12. App bootstrap still panics on expected filesystem setup failures
    Where: crates/agent-canvas-app/src/main.rs:629, :1228
    What's promised vs what exists: Cold start should be low-friction. `bootstrap().expect(...)` crashes on iCloud/symlink/state-db setup failures instead of showing an actionable app error.
    Severity: friction
    Suggested fix scope: medium (~1hr)

Top 5 to fix: First, root-bound exposed document and sidecar commands. Second, tighten artifact identity so same-hash duplicates do not inherit each other’s state. Third, make persona colors registry-driven and reconcile visual-system tokens. Fourth, replace prompt-based conflict/default-agent flows with in-app controls. Fifth, add thin UI/integration smoke coverage for v0.1.1 flows currently marked pass-by-implementation.
