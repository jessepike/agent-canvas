---
project: agent-canvas
spec: v0.5
status: draft
created: 2026-05-22
author: cto
implements: AWS Interaction Protocol Spec v1.1.0 (ADR-003)
supersedes: v0.4 backlog items [interactive-send], [send-dialog]; folds in Slice 9 (loop receipts)
---

# AgentCanvas — Build Spec v0.5: Interaction Protocol Conformance

## Goal

Make AgentCanvas the **reference renderer + structured-return implementation** of the
AWS Interaction Protocol v1. Agents dispatch a *typed interaction* (one of four classes);
Canvas renders the right widget and returns a **structured, machine-actionable response**
correlated by `interaction_id` — never free text, never hand-rolled HTML.

This supersedes the ad-hoc send-back we grew during v0.3–v0.4 (`{action_verb, note}` prose
returned via `get_user_messages`) and the clipboard-paste workaround surfaced in the
2026-05-22 dogfood.

## Authority & boundary (read before building)

- **The protocol is the authority, and it is Forge-owned.** Normative contract lives at
  `~/code/_shared/aws/docs/specs/interaction-protocol/INTERACTION-PROTOCOL-SPEC-v1.md`
  (decision: `aws/docs/decisions/ADR-003-canvas-interaction-protocol.md`). This spec
  **points at** it and must not copy its normative content. If implementation reveals the
  contract needs to change, **propose back to Forge** — do not diverge unilaterally.
- **Canvas implements the contract; it does not own surface-selection or agent wiring.**
  The decision "terminal vs Canvas," the `AskUserQuestion` fallback, and the
  agent-definition / CLAUDE.md rollout are the agent's / Forge's concern (ADR-003 §Rollout).
  Canvas's job: render dispatched interactions, capture structured responses, expose the
  lifecycle. **Canvas must never be a hard dependency** — if it's down, the agent falls back;
  nothing here assumes Canvas is required.
- **Conformance, not invention.** The four classes, request envelope, return contract, and
  hard rules are defined by the protocol spec §2–§4. Build to them; don't add classes
  (propose via ADR).

## Current state → target (what changes)

| Concern | Today (v0.4) | Target (v0.5, per protocol) |
|---|---|---|
| Agent → operator request | `open_artifact` / `notify_user` (no class, no id) | `dispatch_interaction(envelope)` carrying `interaction_id`, `class`, `questions[]`, `trace_id` |
| Operator → agent response | `user_messages` row `{note, action_verb}` (prose) | structured `payload` per protocol §4 (`responses[]` / `comments[]`+`edits[]` / `decision`+`reason`), correlated by `interaction_id` |
| `get_user_messages` shape | `{messages:[{id,session_id,path,note,action_verb,created_at}]}` (v0.4 wrap) | `{messages:[{interaction_id, ts, payload:{§4 incl. class}}]}` (protocol §5, v1.1.0) |
| `get_comments` shape | `{comments:[…sidecar…]}` (v0.4 wrap) | unchanged shape; feeds `document-review` responses |
| Rich UI | agent hand-rolls HTML (clipboard return) | Canvas renders typed classes from the envelope |
| Receipts | none (Slice 9 planned) | Delivered→Read→Responded keyed by `interaction_id` (folded in here) |

The v0.4 array→record wrap (commit `a7d9c44`) is the minimal unblock and stays; v0.5 evolves
the **per-item shape** to the protocol envelope.

## Target architecture

### Data model (backend)
New `interactions` table (mirrors the protocol's request+response, additive migration):

```
interaction_id  TEXT PRIMARY KEY,  -- correlates request ↔ response
session_id      TEXT NOT NULL,     -- dispatching agent session
class           TEXT NOT NULL,     -- decision-set | document-review | approval-gate | visual-artifact
title           TEXT,
artifact_path   TEXT,              -- absolute doc/HTML path (XOR artifact_inline)
artifact_inline TEXT,              -- inline HTML (visual-artifact ONLY; XOR artifact_path)
trace_id        TEXT,              -- echoed to response; spine writes the handoff-event log
request_json    TEXT NOT NULL,     -- the §3 envelope as received
status          TEXT NOT NULL,     -- pending | submitted | draft | dismissed
response_json   TEXT,              -- the §4 structured payload on submit (carries submitted_at ISO-8601)
created_at      INTEGER NOT NULL,  -- epoch, internal ordering only
responded_at    INTEGER,           -- epoch, internal ordering only
read_at         INTEGER            -- epoch; set when the agent consumes it (Read receipt)
+ index on (session_id, status), (interaction_id)
```

**Timestamps (locked by spec v1.1.0):** all WIRE/payload timestamps are **ISO-8601 UTC with
trailing `Z`**. `payload.submitted_at` is canonical; the transport wrapper `ts` mirrors it and
is also ISO-8601 (if they ever disagree, `submitted_at` wins). Table `*_at` columns stay epoch
for internal ordering/indexing only — never emitted raw; derive the ISO-8601 wire values from
them (or store `submitted_at` verbatim in `response_json`).

- `comments` stay in the existing sidecar (document-review pulls them into `response_json`).
- The v0.4 `user_messages` free-text send-back is **deprecated** in favor of `interactions`;
  keep the table for one version for backward read, then remove (Slice 7).
- `agent_messages` (notify) is unchanged — `notify_user` remains the lightweight ping; it is
  NOT an interaction.

### MCP surface
- **New: `dispatch_interaction(envelope)`** — agent → Canvas. Validates the §3 envelope
  (requires `interaction_id`, `class`; `questions[]` for decision-set), inserts an
  `interactions` row (`status=pending`), surfaces it in the UI (renders the class, raises
  window), and pings via the existing notification channel. Returns `{dispatched: true,
  interaction_id}`. **Lock discipline:** insert under the db guard, window/emit post-lock
  (per commits `95261f6`/`cffcae5`).
- **Evolve `get_user_messages`** → exact element shape (spec v1.1.0 §5):
  `{messages:[{ "interaction_id": "...", "ts": "<ISO-8601 Z>", "payload": { /* §4 verbatim:
  interaction_id, class, artifact_path, status, submitted_at + per-class fields */ } }]}`.
  Wrapper `interaction_id` is canonical for routing; `payload` repeats it and **must equal** the
  wrapper (agent validates against `payload.interaction_id`); `ts` mirrors `payload.submitted_at`.
  Canvas MUST NOT add fields to `payload` beyond §4. Return `status in (submitted, draft)`
  interactions for the caller's session (honor `since`). On read, set `read_at` inside the lock
  and emit the `interaction.read` lifecycle event + `messages-changed` post-lock. Non-destructive
  (agents dedupe via `since`).
- **`get_comments`** keeps `{comments:[{anchor, body, ts}]}` (already conformant).
- **Keep** `open_artifact` / `attach_artifact` for the plain "just show me this file" path
  (not every artifact is an interaction) and `get_current_focus`.
- **Version the tool surface** — advertise the implemented protocol version; breaking
  envelope changes bump per protocol §8.

### Renderers (frontend) — one per class
- **`decision-set`** (headline): render `questions[]` (AskUserQuestion-shaped) — radio for
  `multiSelect:false`, checkboxes for true; show option `label`/`description`, a
  `recommended` badge (hint, never pre-selected); per-question note field. Submit builds
  `responses[] = [{question_id, selected:[keys], note}]` — **`selected` is option `key`s, not
  labels; unanswered questions omitted** (protocol §4 rules). This is the typed replacement
  for the hand-rolled HTML form from the dogfood.
- **`approval-gate`**: Approve / Reject / Request-changes + required `reason` on
  reject/request-changes → `{decision, reason}`.
- **`document-review`**: render the doc; reuse existing inline comments + the edit/save flow,
  computing `edits[] = [{kind:"diff", unified_diff}]` from the operator's edits and gathering
  sidecar `comments[]` into the response.
- **`visual-artifact`**: render from `artifact_path` **XOR** `artifact_inline` (inline HTML
  string, valid for this class only — `document-review` always uses `artifact_path`); reuse the
  sandboxed HTML viewer (scripts disabled — script-bearing HTML stays out of scope per protocol
  §9) + annotate → `comments[]`. Escape hatch for genuinely-custom rendering.

### Response builder
The v0.4 "Send to" dialog becomes the **response submitter**: it emits the §4 envelope —
`interaction_id` echoed, **`class` echoed** (spec v1.1.0; lets the agent dispatch without
inferring), `artifact_path`, `status` = submitted|draft|dismissed, `submitted_at` (ISO-8601
UTC `Z`), plus the class-specific fields. No free-text-only path. "Dismiss" sets
`status:dismissed`; "Save draft" sets `status:draft`.

### Lifecycle, receipts, telemetry
- Emit the four Canvas-side lifecycle events (spec v1.1.0 §5.1): `interaction.dispatched`,
  `interaction.read`, `interaction.responded`, `interaction.dismissed` — each carrying
  `interaction_id`, `trace_id`, `ts` (ISO-8601 Z), and `class`/`status` where relevant. These
  drive the **Delivered → Read → Responded** receipt UI (folds in Slice 9), keyed by
  `interaction_id`.
- **Handoff-event log writing is NOT Canvas's job (locked, spec v1.1.0 §7).** The wm-agent
  spine consumes the lifecycle events and writes the boundary record (`human_in_loop`,
  `gate_type`, cost/outcome, `trace_id`). **Canvas's responsibility is exactly two things:**
  (a) echo `trace_id` from request → response untouched, and (b) emit the four lifecycle
  events above. Canvas never writes/reads the handoff-event JSONL.
- **Spine re-invoke** (agent releases → re-invoked on matching `interaction_id`) is the
  spine's job; Canvas's responsibility is only that `get_user_messages` polling works as the
  always-available fallback.

## Slices

1. **Protocol substrate.** `interactions` table + migration; `dispatch_interaction` tool
   (envelope validation, insert, surface, ping); evolve `get_user_messages` to
   `{messages:[{interaction_id, payload, ts}]}` + `read_at` on read. Tests: dispatch→row,
   get→wrapped structured payload, `read_at` set once, lock discipline. **Accept:** an agent
   can dispatch a `decision-set` and read back a structured response by `interaction_id`.
2. **`decision-set` renderer + response builder** (headline). Form from `questions[]`;
   submit builds `responses[]` (keys, omit unanswered); status submitted/draft/dismissed.
   **Accept:** the handoff-schema decisions round-trip works natively — no clipboard, no
   hand-rolled HTML.
3. **`approval-gate` renderer.** approve/reject/request-changes + reason → `{decision, reason}`.
4. **`document-review`.** comments[] (existing sidecar) + edits[] as unified diff from the
   edit/save flow.
5. **`visual-artifact`.** render supplied HTML/image payload + annotate → comments[].
6. **Receipts + lifecycle.** Delivered→Read→Responded keyed by `interaction_id`; lifecycle
   events exposed for the spine; `trace_id` carried through.
7. **Deprecate the ad-hoc path.** Remove free-text `{action_verb, note}` send-back; migrate
   `user_messages` readers; version the tool surface; update `docs/claude-md-template.md` +
   `docs/mcp-clients.md` to the protocol (request via `dispatch_interaction`, structured
   return).

## Key decisions (mini-ADRs)

- **New `interactions` table over extending `user_messages`.** Context: responses are now
  structured + correlated, not prose. Decision: dedicated table holding request+response
  keyed by `interaction_id`; deprecate `user_messages`. Rationale: clean correlation,
  per-class payloads, room for status/read lifecycle. Consequence: a migration + reader
  cutover (Slice 7). Shelf life: revisit if the protocol adds streaming/partial responses.
- **`dispatch_interaction` as a new tool, `open_artifact` retained.** Not every artifact is
  an interaction; plain "view this" stays cheap. Rationale: keeps the lightweight path and
  the typed path distinct. Shelf life: if 95% of opens become interactions, reconsider
  merging.
- **Canvas renders classes; it does not author HTML and does not own fallback/telemetry
  emission.** Per ADR-003. Rationale: typed classes are the whole point; fallback and
  handoff-event writing belong to the agent/spine. Consequence: Canvas exposes lifecycle but
  doesn't write the handoff log.

## Resolved by Forge (protocol v1.1.0, 2026-05-22)

- **`interaction_id`:** wrapper-level is canonical for routing; `payload` repeats it and the
  two MUST be equal; agent validates against `payload.interaction_id`. Wire element is
  `{interaction_id, ts, payload}`.
- **Time format:** ISO-8601 UTC with trailing `Z` everywhere; `payload.submitted_at` canonical,
  wrapper `ts` mirrors it (`submitted_at` wins on conflict).
- **Handoff-event:** the spine writes the log; Canvas only echoes `trace_id` + emits the four
  §5.1 lifecycle events.
- **`class` in response:** ACCEPTED — echo the request class in the §4 response.
- **`visual-artifact` source:** `artifact_path` XOR `artifact_inline`; inline HTML only for
  `visual-artifact`; both sandboxed, scripts disabled.

## Open questions (capture, don't build)

- **`document-review` edit capture:** diff against the dispatched artifact version vs the
  on-disk version at submit (concurrent-edit window). Reuse the existing stat+hash guard.
- One-click "Track this" promotion for ephemeral files (carried from v0.4).

## Acceptance (milestone)

A connected agent dispatches a `decision-set`; Canvas renders the form; the operator selects
options + notes and submits; the agent reads a structured `{responses:[{question_id,
selected:[keys], note}]}` correlated by `interaction_id` — with no clipboard and no
hand-authored HTML. `approval-gate`, `document-review`, `visual-artifact` likewise return
their §4 shapes. Canvas-down degrades to the agent's terminal fallback with no error. Gates:
`cargo test`, `tsc`, `vite build`, A22=0, A15=0.
