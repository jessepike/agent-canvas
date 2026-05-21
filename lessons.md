---
project: vellum
updated: 2026-05-10
---

# Vellum — Lessons (hot buffer)

One line per insight, dated. Keep last 15. Promote cross-project patterns to KB at session wrap.

## 2026-05-10 (scaffold + Gate 30A + Gate 30B start)

- [pending] **Iframe keyboard shortcuts must be bridged inside the iframe.** Parent `document` keydown handlers do not receive `Cmd+Shift+M` when focus is inside an opaque-origin iframe, so the injected srcdoc bootstrap needs its own keydown listener that posts the selected range to the host.
- [pending] **Rollup optional native deps can disappear after host/VM node_modules drift.** Symptom: Vite fails on missing `@rollup/rollup-linux-arm64-gnu`. Fix inside OrbStack dev VM with `CI=true pnpm install --frozen-lockfile`; do not install on host.
- [pending] **Codex sandbox cannot commit (`.git/index.lock` Operation not permitted).** Pattern: Codex does the work, I commit. Tell Codex explicitly "do not commit" in every prompt — otherwise it tries and the failure muddies the report.
- [pending] **Pre-staging the next Codex prompt while the current one runs respects the single-Codex-job constraint.** Only one Codex job at a time (token race). But the orchestrator can DRAFT the next prompt in parallel. Cuts wall-clock between delegations to near zero.
- [pending] **Patch contracts ship in spec BEFORE code touches the IPC.** Codex external review surfaced: if the patch type isn't pinned in the spec, the first implementation pins it accidentally. Decision: BlockPatch struct + BlockEdit enum landed in v0.3 spec §Block patch contract before 30B-00 implementation began.
- [pending] **Edit-source preference rule makes SerializeFromTree the rare path — stub it.** Most edits go through PreservedBytes or EditedBytes branches. Implementing SerializeFromTree fully on day 1 is YAGNI; stub returns `Err(SerializerUnimplemented)`, ship the common path.
- [pending] **Layer A timeline confirmed empirically.** Full Gate 30A (parser + atomic write + watcher + sidecar + auto-migration + tmpfile reaper + corpus 67/67) plus first 30B slice (BlockPatch save flow + UI scaffold + IPC roundtrip) in one day. 7 atomic commits. 2022-era estimate would have been 2-3 weeks. 0.1-0.3x multiplier holding.
- [pending] **Atomic-commit-per-delegation pattern works at Codex pace.** Each Codex delegation → verify in OrbStack/host → commit with detailed Co-Authored-By → push. Avoids end-of-session commit accumulation. Bisectable history. Each commit is a reviewable unit.
- [pending] **My own patches can introduce Criticals.** Cycle 1 patch on §Block identity created the cold-state contradiction. Caught only by cross-model external review. Lesson: internal cycles surface different issues than external; both are needed; verification cycle after every batch of patches is non-optional.
- [pending] **"Locked for scaffolding" doesn't mean review-complete.** A third review pass found 2 Criticals + many Highs that two prior paper reviews missed. Lock the spec for scope-control, not for review-stop.
- [pending] **Spec-vs-code ambiguity is the most common High finding.** Codex caught: prose said "serde ↔ zod via ts-rs" but ts-rs doesn't generate Zod; prose said "trust by handshake name" but handshake-name alone is spoofable; prose said "signed binaries" but signing is multiple operational tracks. Pattern: any time prose describes a concrete mechanism, the mechanism must be specified precisely OR the prose must explicitly name the gap.
- [pending] **Background CLI auth races are real.** Claude -p, Codex, and ChatMock all needed independent reauth — they don't share auth state. Plan for this when running multi-CLI external reviews.
- [pending] **Partition contracts are the right framing for parser correctness.** "Ordered, non-overlapping byte-span partition that reproduces the file" is a CI-testable invariant. Better than "byte-range preservation" as a vague property.
- [pending] **Auto-migration on rename was the right call.** Sidecar identity maps that don't follow `git mv` and Finder rename are useless. Source-hash matching + cache-wide scan on miss is cheap and solves the common case.

## 2026-05-21 (v0.3.0 release wrap)

- [pending] **Tauri `generate_context!` requires RGBA icon PNGs.** `sips` saves RGB by default — proc-macro panics with `icon is not RGBA`. Fix on host: `python3 -c "from PIL import Image; Image.open(src).convert('RGBA').save(dst)"`. Pre-convert source PNG before running the sips+iconutil pipeline.
- [pending] **macOS quarantine xattr blocks first launch of locally-built unsigned .app.** Strip with `xattr -dr com.apple.quarantine /Applications/Foo.app` after copy. Pair with `lsregister -f` to make Spotlight/Launchpad/`open -a` find it. Both are required.
- [pending] **`@tauri-apps/cli` as project-local devDep keeps host clean.** vs `cargo install tauri-cli` which installs to `~/.cargo/bin` host-global. Project-local respects host-protection rule. Path: `cd ui && pnpm add -D @tauri-apps/cli@^2`, invoke via `./ui/node_modules/.bin/tauri build`.
- [pending] **WKWebView context-menu clipping was CSS, not cache.** Spent time clearing `~/Library/WebKit/agent-canvas-app` + `~/Library/Caches/agent-canvas-app` + node_modules/.vite/deps. Actual cause: `.file-context-menu { max-height: 520px; overflow: auto }` hid last 2 entries behind scrollbar. When UI items appear truncated in WKWebView, check CSS `max-height` + `overflow:auto` BEFORE cache-clearing.
- [pending] **`echo | shim` closes stdin and kills MCP shim before push notifications arrive.** Use FIFO: `mkfifo /tmp/x; exec 3<>/tmp/x; shim < /tmp/x > /tmp/y &`. Required for any long-lived stdio MCP smoke test that expects pushed notifications.
- [pending] **macOS fsevents emits multiple events per single write.** Backlogged as v0.3 spinoff (4× notifications per touch). Dedup needs to live at the notification dispatch boundary, not the watcher.
- [pending] **Codex sandbox cannot write `.git/index.lock`.** Recurring across every Codex slice dispatch. Codex finishes implementation, I commit on host. Tell Codex explicitly "do not commit" in every prompt.
- [pending] **Background `nohup launcher > log &` completion notification refers to the launcher, not the long-running child.** When wrapping a long-running command with `run_in_background: true`, the "completed" notification fires when the wrapper bash exits. Use a separate `Monitor` armed against the log to catch the actual termination state. Both terminal states (success + failure) must be in the grep alternation — silence is not success.
- [pending] **Two-stage build with fix-in-the-middle is cheap when cargo's incremental cache holds.** Initial release build failed late (proc macro on icon), but `cargo`/Tauri kept the whole workspace's compile output. After RGBA fix, the resumed build was 9.17s (vs ~10–15 min cold). Pattern: don't fear letting a long build fail if you have a precise next-action; the cache earns it back.
