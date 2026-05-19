# Vellum v0.3 — Review Findings Ledger

**Artifact:** `vellum-spec-v0.3.md`
**Loop start:** 2026-05-09
**Lens:** Engineering integrity & scaffold-readiness
**Stop criterion:** Critical = 0, High = 0
**Out of scope:** Positioning, PMF, GTM, buyer critique (single-user-by-design constraint)

---

## Cycle 1 — Internal Review (CPO + ADF Design dimensions)

| # | Severity | Complexity | Source | Location | Finding | Status |
|---|----------|------------|--------|----------|---------|--------|
| C1 | Critical | **High** | Internal | §Architecture / §Source-preserving save | **Block identity for non-primitive blocks is unspecified.** The existential claim "untouched blocks serialize back as their preserved raw bytes" requires identity that survives edits to neighbor blocks. Any insertion shifts byte ranges of every block after it. Spec says block identity for *primitives* lives in the YAML `id` field — but says nothing about how a paragraph, heading, list, or HTML block is tracked across edits. ProseMirror's tree positions are not stable across structural edits. Without a chosen mechanism (stable IDs decorated onto PM nodes / content-hash per block / event-token pairs / something else), the source-preserving save is a wish. **FLAGGED — needs user decision.** | Open — pending architecture choice |
| C2 | Critical | Medium | Internal | §30/60/90 — Days 1–30 | **Day 30 exit criterion bundles too much under one gate.** Single milestone covers: Tauri scaffold + IPC type generation + format-preservation corpus + custom block parser with byte-range preservation on top of pulldown-cmark + atomic writes + `notify`-backed watcher + external-change diff prompt + conflict-marker detection + ProseMirror custom schema with primitive node types + CodeMirror 6 source view + toggle-time sync — and the exit demands BOTH "corpus passes byte-identical" AND "open and edit a 2MB doc, save, reopen unchanged." Even at Layer A 0.1–0.3× compression the corpus alone is multi-week (HTML blocks, footnotes, link reference definitions, fence-info preservation, BOM, CRLF). One bug in the parser fails both criteria; one missing ProseMirror node type fails the second only. Sets up a milestone that's 90% done and reads as failed. | Open — propose split |
| H1 | High | Medium | Internal | §Source-preserving save / Risk #2 | **`pulldown-cmark` byte-range architecture is named as a risk and not designed.** Risk #2 says "Will need a custom layer on top." That's not a design — that's a TBD on the load-bearing wall. Modern `pulldown-cmark` exposes `OffsetIter` yielding `Range<usize>` per event, but events are tokens, not blocks. Reconstructing block boundaries (HTML blocks, link reference definitions, footnote definitions, lazy continuations inside lists/blockquotes) is the unsolved part. Spec should at minimum: (a) name the algorithm (event-stream block-boundary reconstruction), (b) enumerate the known-hard CommonMark cases the corpus must cover, (c) flag a fallback if pulldown-cmark proves insufficient (tree-sitter-markdown? markdown-rs?). | Open — propose spec patch |
| H2 | High | Low | Internal | §Open source + project setup vs §30/60/90 Days 61–90 | **Public-from-day-1 vs day-90-release contradiction.** Line ~384: "Public GitHub from the first commit. No private prelude." Line ~345: Day 90 milestone deliverable includes "Open-source release: GitHub repo, Apache 2.0…" These are inconsistent. If repo is public from commit 1, day 90 ships a *tagged release*, not the repo. | Open — pick one |
| H3 | High | Low | Internal | §Primitive schema / §Evidence State | **`inline_snapshot` / `vellum:result` block schema unspecified.** Spec references "an adjacent `vellum:result <id>` block" as the freeze-for-archive primitive, and the "Changed-since-frozen" Evidence State requires comparing the snapshot's recorded recipe-hash with the current `content_hash`. Neither the snapshot block schema nor the recipe-hash field on the snapshot is defined. Load-bearing for the honest-history claim. | Open — define schema |
| H4 | High | Medium | Internal | §30/60/90 — Days 31–60 | **MCP auth lifecycle unscoped inside day 60 milestone.** Each MCP server brings its own auth model (OAuth flows, API keys, refresh tokens, device codes). Spec mentions "OS keychain (credentials via MCP server config)" only in the on-disk-semantics section. The day-31–60 milestone says "MCP client integration with capability discovery" but not which server is integrated first or what auth surface is in scope. First real-world server can blow the schedule. | Open — pin first server, scope auth surface |
| M1 | Medium | Low | Internal | §Trust (personal config) | **Trust pattern syntax (`"local-mcp.*"`) unspecified.** Glob? Regex? TOML-key wildcards aren't standard. State whether `*` matches single segment or any-suffix, and what character class is allowed. | Open |
| M2 | Medium | Low | Internal | §Rust core / §File system layer | **Atomic write tmpdir not specified.** "Write-temp + rename" silently degrades to copy when tmpdir is on a different volume than the target file (vault on external drive). Specify same-directory tmpfile (e.g., `<doc>.vellum-tmp-<pid>` next to target), then atomic `rename(2)`. | Open |
| M3 | Medium | Low | Internal | §File and on-disk semantics | **Cache directory location ambiguous.** Spec says sibling `.vellum-cache/<docname>/`. Sibling to the doc, or sibling to vault root with per-doc subfolders? Different ergonomic and `.gitignore` implications. Pick one. | Open |
| M4 | Medium | Medium | Internal | §Trust (personal config) | **MCP destination metadata source not defined.** `[destinations]` is keyed by provider (`anthropic`, `openai`), implying Vellum knows where each tool sends data. MCP doesn't surface destination metadata uniformly. Either (a) infer from server identity, (b) require manifest declaration, or (c) drop destination tier and just gate on tool. | Open |
| M5 | Medium | Low | Internal | §Primitive schema | **`created_at` listed as auto-managed but example shows it inline.** Spec says `Auto-managed: content_hash, last_run_at, created_at` — but `created_at` is set on creation, not updated on run. Either move it out of auto-managed or clarify "auto-injected on first save." | Open |
| M6 | Medium | Low | Internal | §Anti-patterns vs §Trust | **"Click-through security UX" anti-pattern conflicts with `ask` trust state.** `ask` triggers a prompt per invocation — that's textbook click-through unless the prompt carries enough rationale + state to be a real decision. Define what makes "ask" not click-through (e.g., shows tool name + args summary + one-time vs persistent grant). | Open |
| L1 | Low | — | Internal | §Performance envelopes | Performance targets ("cold start <1.5s", "open 1MB doc <300ms") set without baseline measurement on Tauri+empty-project. Aspirational targets in a locked spec become failure-by-spec. Demote to "post-day-30 validated budget." | Logged, no fix |
| L2 | Low | — | Internal | §Primitive schema | v1 schema doesn't reserve namespace for v1.5+ primitive types (`vellum:agent`, `vellum:transform`, `vellum:include`). Adding a `kind:` discriminator now is cheaper than later. | Logged, no fix |
| L3 | Low | — | Internal | §The UI shell | "ProseMirror custom schema with primitive node types" is the unit of design and is abstracted. v1.5 primitives will force schema changes; flag now that the schema is the API surface. | Logged, no fix |

### Cycle 1 actions

- **C1** — RESOLVED. User picked Option A (PM-decorated stable IDs). Spec patched with new §Block identity section.
- **C2** — RESOLVED. Day 30 milestone split into Gate 30A (parser + corpus) and Gate 30B (editor end-to-end).
- **H1** — RESOLVED. New §Block boundary reconstruction subsection specifies the algorithm, hard cases, and fallback to `markdown-rs`.
- **H2** — RESOLVED. Day 90 milestone reframed as "v1.0 tagged release" on the already-public repo.
- **H3** — RESOLVED. New §`vellum:result` subsection with full schema.
- **H4** — RESOLVED. First MCP server pinned (GitHub, read-only); auth surface scoped (API token via OS keychain); OAuth deferred to v1.5.
- **M1–M6** — RESOLVED. Trust pattern syntax, atomic write tmpdir (same-volume), cache directory (vault-rooted), MCP `[destinations]` → `[servers]` rename + clarification, `created_at` field-status, `ask` rationale wording.
- **L1, L2, L3** — logged, no fix.

---

## Cycle 2 — Internal Review (consistency pass after cycle 1 edits)

| # | Severity | Complexity | Source | Location | Finding | Status |
|---|----------|------------|--------|----------|---------|--------|
| H5 | High | Low | Internal | Multiple | **Identity.json path inconsistency** — three locations referenced `.vellum-cache/<doc>/identity.json` (old) while spec body now mandates `<vault-root>/.vellum-cache/<docpath>/`. | Fixed |
| H6 | High | Low | Internal | §Primitive schema → result_policy | **`pinned` description** still used `.vellum-cache/<docname>/<id>.json` (old format). Updated to vault-rooted `<docpath>`. | Fixed |
| H7 | High | Low | Internal | §Risks accepted Risk #2 | **Risk #2 was stale** — described the parser layer as "Will need a custom layer on top" (TBD) when the layer is now designed in §Block boundary reconstruction. Risk reframed around residual edge cases + fallback. | Fixed |
| H8 | High | Low | Internal | §Delta from v0.2 vs Risk #6 | **Doctor contradiction.** Delta said "demoted to lightweight scan/repair panel"; Risk #6 said "No `vellum doctor` in v1." Resolved: doctor cut from v1; inline validation replaces it; reconsidered post-v1. | Fixed |
| M7 | Medium | Low | Internal | §Sign-off | "No third paper review" framing was stale. Updated to acknowledge cycle 3. | Fixed |
| M8 | Medium | Low | Internal | §Risks preamble | "In place of a third paper review round" framing was stale. Removed. | Fixed |
| M9 | Medium | Low | Internal | §Source-preserving save Architecture bullets | Mild redundancy with new §Block identity section (bullets describe WHAT, identity section describes HOW). Left intact — complementary, not contradictory. | Logged, no fix |
| M10 | Medium | Low | Internal | §Primitive schema → result_policy `inline_snapshot` | Description referenced `vellum:result <id>` block — actual schema uses `vellum:result` tag with `for_id` field inside. | Fixed |
| M11 | Medium | Low | Internal | §Primitive validation | "Stale `content_hash`" wording was misleading — `content_hash` is computed from recipe, not result. Rewrote: "last-run result older than the primitive's `cache` window" is the stale-cache signal; `content_hash` mismatch is the recipe-edited signal. Added orphan-snapshot validation. Added paste-id-regenerate affordance. | Fixed |
| M12 | Medium | Low | Internal | §When external changes happen | Three-pane diff "(in-memory, on-disk, base)" was undefined. Added pane definitions and noted the freshly-opened-doc 2-pane degenerate case. | Fixed |
| M13 | Medium | Low | Internal | §`vellum:result` | Snapshot lifecycle on recipe edit was unspecified (kept? deleted? banner?). Specified: kept on disk, surfaces as `Changed-since-frozen` until user re-freezes or removes. | Fixed |
| L4 | Low | Low | Internal | §Trust pattern syntax | Tiebreaker for equally-specific patterns: order-of-declaration in the TOML file. | Fixed |
| L5 | Low | Low | Internal | §Rust core crate list | Crate versions not pinned. Added floor-version pins for load-bearing libs (`tauri 2.x`, `pulldown-cmark 0.13+` for OffsetIter, etc.). | Fixed |

### Cycle 2 actions

- All H5–H8 fixed.
- All M7, M8, M10–M13 fixed.
- M9 logged, no fix (mild duplication, not contradiction).
- L4, L5 fixed (low cost).

---

## Cycle 3 — Verification Pass

| # | Severity | Complexity | Source | Location | Finding | Status |
|---|----------|------------|--------|----------|---------|--------|
| F1 | Low | Low | Internal | §Rust core / Trust manager | Bullet still said "which destinations are allowed" — stale after [destinations]→[servers] rename. Updated. | Fixed |
| F2 | Medium | Low | Internal | §Primitive schema | `content_hash` computation was undefined. Specified: `blake3(version \|\| tool \|\| canonical_yaml(args))`, excludes auto-managed fields and cache window. | Fixed |
| F3 | Low | Low | Internal | §Primitive schema | `cache: 60s` duration parsing rules undefined. Specified: Go-style durations; integer rejected; `0s` = always-rerun; missing = cache-forever. | Fixed |
| F4 | Medium | Low | Internal | §`vellum:result` lifecycle | Ambiguity between manual-freeze lifecycle and `result_policy: inline_snapshot` lifecycle. Resolved: manual-freeze = one-shot, never auto-overwritten, supports `Changed-since-frozen`. inline_snapshot policy = overwritten on every successful run, no `Changed-since-frozen`. | Fixed |
| F5 | Medium | Low | Internal | §Evidence State | Frozen/Changed-since-frozen language rewritten to compare snapshot's `recipe_hash` against primitive's `content_hash` (consistent with the new schema), and to note that `inline_snapshot` policy doesn't surface Changed-since-frozen. | Fixed |

### Cycle 3 actions

- All 5 fixes applied.
- **Phase 1 internal review exits clean: Critical=0, High=0.**

---

## External Review — Codex CLI

Completed after reauth. 151,843 input / 3,102 output tokens. 2 Critical + 5 High + 0 Low.

| # | Severity | Complexity | Source | Location | Finding | Status |
|---|----------|------------|--------|----------|---------|--------|
| Y-C1 | **Critical** | Medium | External-Codex | §Block identity / Gate 30A | **Cold-state contract internally inconsistent.** Same Critical as X-C1 from Claude -p — both reviewers converged independently. Codex framing: line 112 "every block re-serializes through the Markdown serializer" cannot coexist with Gate 30A "byte-identical with sidecar absent" for arbitrary Markdown. **STRONG SIGNAL — cross-model convergence on a Critical.** | Open (merge with X-C1) |
| Y-C2 | **Critical** | Medium | External-Codex | §Block boundary reconstruction (lines 136-143) | **Block partition is not specified as non-overlapping.** Treating `List`, `Item`, `BlockQuote`, and nested starts as independent block frames risks overlapping byte ranges. Link reference definitions are flagged as hard cases but aren't normal block start/end events in `pulldown-cmark`. Without a non-overlapping top-level partition contract, the parser is not scaffold-ready. | Open |
| Y-H1 | High | Medium | External-Codex | §Block identity, §Architecture, §UI shell sync, §30/60/90 | **PM ↔ Rust patch contract too implicit.** "Content and structure unchanged" is not a concrete equality rule between PM nodes and raw Markdown blocks. "Changed IDs are re-serialized" is dangerous because editing one inline token can cause whole-block formatting loss. This is exactly where corruption bugs concentrate. | Open |
| Y-H2 | High | Low | External-Codex | §Primitive schema, §Evidence State, Days 61-90 | **`content_hash` is both auto-managed persisted field AND live recipe identity.** If recipe is edited but not rerun, stored field is stale → breaks `vellum:result.recipe_hash` comparisons. Plus same wording ambiguity Claude -p caught: line 416 "writes adjacent `inline_snapshot` block" should be `vellum:result` block. **Partial convergence with X-H5 / X-L2.** | Open (merge) |
| Y-H3 | High | Low | External-Codex | §Trust (lines 291-322) | **MCP server identity is spoofable.** Trusting servers by handshake-advertised name lets a malicious/misconfigured local MCP server claim `github` and inherit existing trust. Plus same wildcard contradiction Claude -p caught (`.*` rule says single-component but example is multi-component). **Partial convergence with X-H3; spoofability is NEW.** | Open |
| Y-H4 | High | Medium | External-Codex | §Rust core file system, §30/60/90 (lines 176, 382-388, 411-420) | **Save preconditions underspecified — corruption guard missing.** Watcher events can be missed/delayed. `rename(2)` can overwrite a newer file unless save-path stats/hashes the target immediately before commit and aborts to three-way conflict flow if it differs from base. Not a feature — a corruption guard. | Open |
| Y-H5 | High | Low | External-Codex | Tauri scaffold + signing (lines 184, 382, 420, 455-461) | **Scaffold pins/release assumptions are traps.** (a) `ts-rs` generates TypeScript types, NOT Zod validators — "serde ↔ zod contract types via `ts-rs`" stalls unless another generator is chosen. (b) Day-90 "signed binaries" hides macOS notarization + Windows code-signing as separate operational tracks, not build flags. | Open |

**Codex positive notes (carry-forward, not findings):** Spec is "compact and reasonably disciplined" but locked-language outpaces implementation contract in several places.

## External Review — Claude -p (Opus 4.7)

Completed in 102s, $0.52, 5705 output tokens.

| # | Severity | Complexity | Source | Location | Finding | Status |
|---|----------|------------|--------|----------|---------|--------|
| X-C1 | **Critical** | Medium | External-Claude | §Block identity (line 112) vs Gate 30A exit criterion | **Cold-state corpus gate contradicts identity-loss fallback.** §Block identity says loss of sidecar map → "every block re-serializes through the Markdown serializer on next save." Gate 30A demands corpus pass byte-identical at the cold-state gate (sidecar absent). A no-edit open→save in cold state would route every non-primitive block through the serializer — which cannot be guaranteed byte-identical. Mutually exclusive as written. (My own cycle-1 patch introduced this contradiction.) | Open |
| X-H1 | High | Low | External-Claude | §Primitive schema | `result_policy` listed as both required AND default `pinned`. A field cannot be both. | Open |
| X-H2 | High | Low | External-Claude | §Trust | **Cross-table precedence between `[tools]` and `[servers]` unspecified.** "Most-specific match wins" is stated only within a table. What if `[servers] github = "block"` and `[tools] "github.list_issues" = "trusted"`? | Open |
| X-H3 | High | Low | External-Claude | §Trust pattern syntax | **Pattern rule contradicts its own example.** Rule says single-namespace-component; example shows `local-mcp.foo.bar` (two components past `local-mcp`). | Open |
| X-H4 | High | Low | External-Claude | §Trust defaults vs Days 31–60 | **First-trust unreachable for new servers.** `unknown_server = "block"` blocks any unknown MCP server outright, but milestone promises "trust prompts on first call to a new tool." If server is unknown, call is blocked before the tool prompt fires. Untrusted state UI only addresses tool axis. | Open |
| X-H5 | High | Low | External-Claude | §`vellum:result` lifecycles vs Days 61–90 freeze | **Manual freeze on an `inline_snapshot` primitive is undefined.** Both lifecycles share the same on-disk block shape with no field distinguishing them. Reject? Convert? Sibling? | Open |
| X-H6 | High | Low | External-Claude | §`vellum:result` schema | **`data` payload type undefined across `render` modes.** Always JSON? For `render: markdown`, raw text? For `render: metric`, a number? Affects round-trip determinism and `result_hash`. | Open |
| X-H7 | High | Low | External-Claude | §`vellum:result` / Hash engine | **`result_hash` computation never defined.** `content_hash` has explicit formula; `result_hash` is referenced (in result block, Evidence State expand panel) but undefined. Without it, frozen-vs-refreshed diff and run log are non-reproducible. | Open |
| X-L1 | Low | Low | External-Claude | §Glossary | Glossary lists `Changed` state; body uses `Changed-since-frozen`. Same state, two names. | Open |
| X-L2 | Low | Low | External-Claude | Days 61–90 freeze bullet | Conflates policy value (`inline_snapshot`) with block type (`vellum:result`). | Open |
| X-L3 | Low | Low | External-Claude | §Primitive schema vs §Evidence State | `cache` absent ("forever") never produces Cached state; `cache: 0s` never produces Live across visibility. State machine doesn't acknowledge edge values. | Open |
| X-L4 | Low | Low | External-Claude | §Rust core atomic write | Tmpfile `<doc>.vellum-tmp-<pid>` cleanup undefined. Crash leaves leftovers visible to ls/sync clients. | Open |
| X-L5 | Low | Low | External-Claude | §`vellum:result` placement | "Adjacent" placement asserted but not enforced. Non-adjacent `vellum:result` (user moved it) — valid? Frozen state still resolves? | Open |

**Reviewer's positive notes (carry-forward, not findings):**
- Milestone honesty plausible at Layer A velocity for the work described. 30A/30B split is the right call.
- Risk surface honesty: §Known risks names the real risks. Worth folding the Critical/High findings above into that section once resolved.

## External Review — Codex CLI

(pending Phase 1 clean exit)

## External Review — Claude -p

(pending Phase 1 clean exit)

## External Review — Claude -p (round 2, post-spec edits but pre-external-batch)

Completed 97s, $0.51, regression-free at Critical level. 0 Critical + 3 High + 6 Low. Most findings unique to round 2, surfacing issues round 1 didn't.

| # | Severity | Source | Location | Finding | Status |
|---|----------|--------|----------|---------|--------|
| Z-H1 | High | External-Claude-2 | §Trust + Days 31-60 | **Server gate vs tool gate evaluation order undefined.** Same root issue as X-H4 + Y-H3 trust evaluation. Three reviewers converged. | Open (merge with X-H4) |
| Z-H2 | High | External-Claude-2 | §Block identity + cache layout | **Sidecar identity map keyed by `<docpath>` — rename/move semantics unspecified.** Common case (Finder rename, `git mv`) drops to cold state. | Open (decision: auto-migrate) |
| Z-H3 | High | External-Claude-2 | §Primitive schema, §Block identity | **Primitive identity before first save undefined.** Freshly-typed primitive has neither YAML id nor PM UUID. | Open (decision: auto-inject on first save) |
| Z-L1 | Low | External-Claude-2 | §Block identity (line 110) | "for changed IDs, the block is re-serialized" — UUIDs don't change; what changes is the *content* of a node with a known ID. | Open |
| Z-L2 | Low | External-Claude-2 | §Primitive schema | `render` field's required/optional/default status unspecified. | Open |
| Z-L3 | Low | External-Claude-2 | §`vellum:result` + §Primitive validation | Manual freeze on `inline_snapshot` primitive undefined; collides with "two results per for_id" hard error. | Open |
| Z-L4 | Low | External-Claude-2 | §`vellum:result` | `data` schema for non-JSON renderers (markdown, metric) unspecified. | Open |
| Z-L5 | Low | External-Claude-2 | §Primitive schema | `created_at` injection conditions implied but not stated (first run vs first save; user-authored preserved or overwritten). | Open |
| Z-L6 | Low | External-Claude-2 | §Block boundary reconstruction | Walker doesn't disambiguate nested events (List vs Item, BlockQuote contents). | Open |

**Reviewer's positive notes:** "Spec is otherwise architecturally coherent: the two-identity-system is sound, the source-preserving contract is the right load-bearing wall to put first, the 30A/30B split honors that, and milestone work matches the described scope. No Critical findings."

---

## Cycle 4 — Synthesis Patch + Verification

**Three user decisions taken (all Recommended):**
1. Y-H1 → New §Block patch contract subsection (~30 lines, includes Rust struct, edit-source preference rule, merge/split/delete semantics, save flow, error model).
2. Z-H2 → Auto-migrate sidecar by source-hash on rename/move.
3. Z-H3 → Auto-inject `id` on first save (treated like `created_at`).

**All findings resolved or merged:**

| Finding(s) | Resolution |
|------------|------------|
| **X-C1 + Y-C1** (cold-state contradiction) | New §Block identity rewrite: cold-state preserves raw bytes from parser (no identity map needed for byte-preservation on no-op round trip). Both gates now reachable. |
| **Y-C2** (block partition non-overlap) | New "Partition contract" section in §Block boundary reconstruction with explicit non-overlap invariant + CI invariant test. |
| **X-H1** (result_policy required+default) | Moved to "Optional with defaults: result_policy (default pinned)". |
| **X-H2 + X-H4 + Y-H3 + Z-H1** (trust precedence + first-trust + spoofing + wildcard) | Full §Trust rewrite: `bind` field for anti-spoofing, deterministic 3-step evaluation order, default `unknown_server = "ask"`, wildcard rule clarified to any-trailing-components, cross-table precedence stated. |
| **X-H3** (wildcard rule contradicts example) | Resolved by making rule "any number of trailing components." |
| **X-H5 + Y-H2 + Z-L3** (manual freeze on inline_snapshot lifecycle) | Added explicit "manual freeze unavailable when result_policy: inline_snapshot" rule. |
| **X-H6 + Z-L4** (data payload type) | `data` is canonical-JSON encoded regardless of `render`; per-renderer interpretation specified. |
| **X-H7** (result_hash undefined) | `result_hash = blake3(canonical_json(tool_response))`, computed in Rust core before renderer. |
| **Y-H1** (PM↔Rust patch contract too implicit) | New §Block patch contract subsection. |
| **Y-H2a** (content_hash stale) | Added: persisted hash is cached echo; live recipe hash is authoritative. |
| **Y-H4** (save preconditions / corruption guard) | Added stat+hash precondition before rename(2); abort to three-way conflict if differs. Added pid+UUID tmpfile naming + cleanup-on-vault-open. |
| **Y-H5a** (ts-rs not Zod) | Gate 30A bullet: `ts-rs` for TS types + handwritten Zod schemas; CI type-tests. |
| **Y-H5b** (signing operational tracks) | Day-90 bullet: ad-hoc/dev-signed only; notarization + code-signing deferred to v1.1; README documents workarounds. |
| **Z-H2** (sidecar rename) | Auto-migrate by source-hash. |
| **Z-H3** (primitive id pre-save) | Auto-inject on first save; non-blocking inline hint. |
| **X-L1** (Glossary "Changed" vs body "Changed-since-frozen") | Glossary updated to "Changed-since-frozen". |
| **X-L2** (milestone freeze bullet) | "writes adjacent `vellum:result` block (manual-freeze lifecycle, never auto-overwritten)". |
| **X-L3** (cache edge values in state machine) | Added: no-cache stays Live until recipe change/manual refresh; `cache: 0s` is Live only during visible run frame. |
| **X-L4** (tmpfile cleanup) | Reaped on vault open. |
| **X-L5** (vellum:result placement) | Added: resolution is doc-scoped, not adjacency-scoped. |
| **Z-L1** (sloppy "changed IDs") | Reworded; new "Branch resolution rule on save" section uses precise three-branch language. |
| **Z-L2** (render field status) | Moved to "Optional with defaults: render (default `json`)"; unknown values warn. |
| **Z-L5** (created_at injection) | Same rule as primitive id: auto-inject if absent on first save; user-authored preserved verbatim. |
| **Z-L6** (nested block events) | Resolved in partition contract: outermost frame owns the byte range; inner frames inform PM tree only. |

**Cycle 4 cascade-finding:**
- F6: Risk #6 stale ("No `vellum doctor` in v1 means rough edges...") — doctor was cut, not deferred. Reframed.
- F7: §Delta from v0.2 still mentioned "dataflows" in trust description — fixed to mention servers.
- Added Risk #7 (server-identity binding is best-effort) + Risk #8 (v1.0 unsigned/ad-hoc-signed) capturing residual risks of the patches.

---

## Final Verdict

**Status: READY FOR SCAFFOLDING.**

- Internal cycles: 4 (3 review + 1 verification).
- External rounds: 2 (Codex + Claude -p×2).
- Cross-model convergence: strong on cold-state contradiction (both reviewers independently flagged), trust-precedence ambiguity (3 reviewers converged), and lifecycle wording drift.
- Critical: 0 (was 2 from external; both resolved).
- High: 0 (was 12 unique after merge; all resolved).
- Low: all 14 resolved.
- Stop reason: clean exit, no Critical/High.

**Spec changes applied across cycle 4:**
- §Block identity: ~40 lines added (sidecar migration, cold-state behavior, branch rule, primitive id lifecycle).
- §Block patch contract: ~25 lines added (NEW subsection, Rust struct, edit-source preference, merge/split/delete, save flow, error model).
- §Block boundary reconstruction: ~10 lines added (partition contract + CI invariant).
- §Rust core file system: ~5 lines added (corruption guard, tmpfile cleanup).
- §Primitive schema: ~10 lines reorganized (field statuses, cache duration semantics, content_hash authority).
- §`vellum:result`: ~15 lines added (data encoding, result_hash formula, resolution doc-scoping, manual-freeze guard).
- §Trust: ~25 lines added (bind field, evaluation order, defaults, wildcard semantics).
- §Evidence State: ~3 lines refined (cache edge values).
- §30/60/90: ~5 lines refined (Gate 30A IPC types, Day-90 signing posture).
- §Risks: 2 new (Risk #7 binding-is-best-effort; Risk #8 unsigned-v1.0).
- §Glossary: 1 fix.

**Net effect:** the spec is roughly 100 lines longer (~617 vs ~520 at start). The growth is concentrated in: block identity model, IPC patch contract, trust evaluation. All three were already named risks in v0.3; cycle 4 turned prose risks into specifications.

**Recommendation:** scaffold begins. Path order per spec:
1. `crates/vellum-corpus/` — assemble the corpus first.
2. `crates/vellum-core/src/parse/` — partition-contract parser; gate 30A.
3. The rest in milestone order.
