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
