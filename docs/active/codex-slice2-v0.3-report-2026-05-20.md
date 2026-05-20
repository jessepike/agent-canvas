# Codex Slice 2 v0.3 Report — 2026-05-20

## 1. Files Modified

- `Cargo.lock`
- `Cargo.toml`
- `crates/agent-canvas-app/Cargo.toml`
- `crates/agent-canvas-app/src/main.rs`
- `crates/agent-canvas-app/tauri.conf.json`
- `crates/vellum-core/src/sidecar/mod.rs`
- `ui/src/App.tsx`
- `ui/src/htmlBootstrap.ts`
- `ui/src/types/blocks.ts`
- `ui/src/types/generated/CommentAnchor.ts`
- `BACKLOG.md`
- `lessons.md`
- `status.md`
- `docs/active/codex-slice2-v0.3-report-2026-05-20.md`

## 2. Migration / Schema Changes

`CommentAnchor` is now a backward-compatible union:

```ts
type CommentAnchor =
  | { kind?: "text_selection"; block_id: string | null; start_offset: number; end_offset: number }
  | { kind: "html_selection"; start_offset: number; end_offset: number; snapshot_text: string };
```

Rust mirrors this in `vellum-core::sidecar` with an untagged enum:

- `TextSelection(TextCommentAnchor)` keeps `kind` optional and defaults missing `kind` during deserialization.
- `HtmlSelection(HtmlCommentAnchor)` requires `kind: "html_selection"` and `snapshot_text`.
- Legacy comment sidecars without `kind` continue to deserialize as text selections.

Added app-level tests:

- `legacy_comment_anchor_deserializes_as_text_selection`
- `html_comment_anchor_round_trips_with_snapshot_text`

## 3. Plugin / Config Additions

Added:

```toml
tauri-plugin-persisted-scope = { version = "2", features = ["protocol-asset"] }
```

Registered in the Tauri builder:

```rust
.plugin(tauri_plugin_persisted_scope::init())
```

Enabled `tauri` feature:

```toml
tauri = { workspace = true, default-features = false, features = ["wry", "protocol-asset"] }
```

Added `app.security.assetProtocol` in `tauri.conf.json`:

```json
"security": {
  "assetProtocol": {
    "enable": true,
    "scope": {
      "allow": ["$HOME/**/*"],
      "deny": [
        "/etc/**/*",
        "/System/**/*",
        "/private/etc/**/*",
        "/private/var/**/*",
        "/usr/**/*",
        "/var/**/*",
        "/Library/Application Support/AgentCanvas/**/*",
        "/Library/Application Support/Apple/**/*",
        "$HOME/Library/Application Support/AgentCanvas/**/*",
        "$HOME/Library/Application Support/com.apple/**/*"
      ]
    }
  }
}
```

No capability grant was added for persisted scope. Tauri rejected `persisted-scope:default` as an unknown permission during build; the plugin registers cleanly without a capability entry.

Reference checked: https://v2.tauri.app/es/security/asset-protocol/

## 4. New Host Bootstrap Script

```ts
export const BOOTSTRAP_SCRIPT = String.raw`
(function () {
  const protocol = "agentcanvas-iframe/1";
  const highlightClass = "agentcanvas-comment-highlight";
  let selectionTimer = 0;

  function post(type, payload) {
    window.parent.postMessage(Object.assign({ type, protocol }, payload || {}), "*");
  }

  function stringify(value) {
    if (typeof value === "string") {
      return value;
    }
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  function installStyle() {
    if (document.getElementById("agentcanvas-bootstrap-style")) {
      return;
    }
    const style = document.createElement("style");
    style.id = "agentcanvas-bootstrap-style";
    style.textContent = [
      ":root { --agentcanvas-comment-highlight-bg: Mark; }",
      "." + highlightClass + " { background: var(--agentcanvas-comment-highlight-bg); color: inherit; }"
    ].join("\n");
    document.head ? document.head.appendChild(style) : document.documentElement.appendChild(style);
  }

  function textOffsetForRangePoint(root, targetNode, targetOffset) {
    const prefix = document.createRange();
    prefix.selectNodeContents(root);
    prefix.setEnd(targetNode, targetOffset);
    return prefix.toString().length;
  }

  function publishSelection() {
    const range = currentSelectionRange();
    if (range) {
      post("agentcanvas:selection", { range });
    }
  }

  function currentSelectionRange() {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
      return null;
    }
    const range = selection.getRangeAt(0);
    const body = document.body;
    if (!body || !body.contains(range.commonAncestorContainer)) {
      return null;
    }
    const startOffset = textOffsetForRangePoint(body, range.startContainer, range.startOffset);
    const endOffset = textOffsetForRangePoint(body, range.endContainer, range.endOffset);
    return {
      startOffset: Math.min(startOffset, endOffset),
      endOffset: Math.max(startOffset, endOffset),
      text: selection.toString()
    };
  }

  function findTextRange(text) {
    if (!text || !document.body) {
      return null;
    }
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const nodes = [];
    let fullText = "";
    let node = walker.nextNode();
    while (node) {
      const value = node.nodeValue || "";
      nodes.push({ node, start: fullText.length, end: fullText.length + value.length });
      fullText += value;
      node = walker.nextNode();
    }
    const start = fullText.indexOf(text);
    if (start === -1) {
      return null;
    }
    const end = start + text.length;
    const startEntry = nodes.find(function (entry) {
      return start >= entry.start && start <= entry.end;
    });
    const endEntry = nodes.find(function (entry) {
      return end >= entry.start && end <= entry.end;
    });
    if (!startEntry || !endEntry) {
      return null;
    }
    const range = document.createRange();
    range.setStart(startEntry.node, start - startEntry.start);
    range.setEnd(endEntry.node, end - endEntry.start);
    return range;
  }

  function scrollToSnapshot(text) {
    const range = findTextRange(text);
    if (!range) {
      return false;
    }
    const mark = document.createElement("mark");
    mark.className = highlightClass;
    try {
      range.surroundContents(mark);
    } catch {
      mark.appendChild(range.extractContents());
      range.insertNode(mark);
    }
    mark.scrollIntoView({ block: "center", inline: "nearest", behavior: "smooth" });
    window.setTimeout(function () {
      mark.replaceWith.apply(mark, Array.from(mark.childNodes));
    }, 1500);
    return true;
  }

  ["log", "info", "warn", "error"].forEach(function (level) {
    const original = console[level];
    console[level] = function () {
      const args = Array.from(arguments);
      post("agentcanvas:console", { level, message: args.map(stringify).join(" ") });
      original.apply(console, args);
    };
  });

  window.agentcanvas = {
    protocol,
    sendBack: function (payload) {
      post("agentcanvas:send_back", { payload: payload || {} });
      console.info("agentcanvas.sendBack received by host bridge");
    },
    scrollToSnapshot
  };

  document.addEventListener("selectionchange", function () {
    window.clearTimeout(selectionTimer);
    selectionTimer = window.setTimeout(publishSelection, 80);
  });
  document.addEventListener("keydown", function (event) {
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "m") {
      const range = currentSelectionRange();
      if (range) {
        event.preventDefault();
        post("agentcanvas:comment_shortcut", { range });
      }
    }
  });
  window.addEventListener("message", function (event) {
    if (event.data && event.data.type === "agentcanvas:scroll_to") {
      scrollToSnapshot(event.data.text || "");
    }
  });
  installStyle();
}());
`;
```

Injection is prefix-based via `srcDoc`, inserted immediately after `<head>` when present. This avoids `iframe.contentWindow.eval`, which is not viable with the required opaque-origin iframe.

## 5. Tests Added

- Rust unit test for legacy comment sidecar migration behavior.
- Rust unit test for HTML comment anchor round-trip behavior.
- Manual invariant audits for A22, A15, and A17.

## 6. Verification Results

Command:

```bash
cd crates/agent-canvas-app && cargo check -q
```

Output:

```text
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.
```

Result: pass, exit 0. Warning is from `ts-rs`; serde still applies the attribute.

Command:

```bash
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -20
```

Output:

```text
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.
   Compiling agent-canvas-app v0.2.2 (/mnt/mac/Users/jessepike/code/sandbox/agent-canvas/crates/agent-canvas-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.20s
     Running unittests src/main.rs (/mnt/mac/Users/jessepike/code/sandbox/agent-canvas/target/debug/deps/agent_canvas_app-53a01a6b5464f8ba)

running 10 tests
test tests::html_comment_anchor_round_trips_with_snapshot_text ... ok
test tests::send_payload_omits_empty_note_and_defaults_action ... ok
test tests::test_path_safe_for_canvas_allow_deny_matrix ... ok
test tests::test_path_within_canvas_resolves_symlinks ... ok
test tests::send_payload_uses_relative_path_fence_note_and_action ... ok
test tests::legacy_comment_anchor_deserializes_as_text_selection ... ok
test tests::test_path_within_canvas_shim_accepts_safe_path ... ok
test tests::migration_backfills_legacy_tags_idempotently ... ok
test tests::untrack_keeps_file_delete_from_disk_removes_file ... ok
test tests::test_identity_relink_skips_when_old_path_exists ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Command:

```bash
cd ui && ./node_modules/.bin/tsc --noEmit
```

Output: no output. Result: pass, exit 0.

Command:

```bash
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5
```

Output:

```text
(!) Some chunks are larger than 500 kB after minification. Consider:
- Using dynamic import() to code-split the application
- Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 1.27s
```

Result: pass, exit 0.

Additional:

```bash
cargo fmt --all --check
```

Result: pass, exit 0.

## 7. Invariant Audit

A22 forbidden flags:

```bash
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l
```

Output:

```text
       0
```

A22 exact sandbox string:

```bash
grep -rn 'sandbox="allow-scripts allow-forms allow-popups allow-downloads"' ui/src/ | wc -l
```

Output:

```text
       1
```

A15 raw hex outside `:root`:

```bash
awk 'BEGIN{inroot=0} /^:root[[:space:]]*\{/ {inroot=1} inroot && /^}/ {inroot=0; next} !inroot && /#[0-9A-Fa-f]{3,8}/ {print FILENAME ":" FNR ":" $0}' ui/src/*.css ui/src/**/*.css 2>/dev/null
```

Output: no output. New iframe highlight uses `var(--agentcanvas-comment-highlight-bg)` inside injected CSS, not a raw hex literal.

A17 native dialogs:

```bash
rg -n 'window\.prompt|window\.confirm|window\.alert|prompt\(|confirm\(|alert\(' ui/src/
```

Output:

```text
ui/src/App.tsx:2657:            confirm();
```

This is an internal React callback named `confirm` in the custom default-agent dialog, not `window.confirm`. No `window.prompt`, `window.confirm`, or `window.alert` host calls were added.

## 8. Known Issues / Gaps For Host Verification

- I did not run a live GUI smoke test in the macOS Tauri window. The code path is verified by compile/build/tests and invariant greps.
- `cargo check -q` and the test tail include a non-fatal `ts-rs` warning for `serde(skip_serializing_if)`. Backlog item added: `[v0.3-slice2-spinoff]` to clean up the generated binding strategy without changing legacy sidecar serialization.
- Vite initially failed because the dev-VM `node_modules` was missing Rollup's Linux optional native package. Fixed inside OrbStack with `CI=true pnpm --ignore-workspace install --no-frozen-lockfile`; no host install was performed.
- Public `fetch()` from iframe content still depends on target API CORS policy. The sandbox allows scripts; CORS remains browser-enforced.
- Commit attempt failed in Codex sandbox: `fatal: Unable to create '/Users/jessepike/code/sandbox/agent-canvas/.git/index.lock': Operation not permitted`. Use the requested commit message from a shell with `.git` write access.
