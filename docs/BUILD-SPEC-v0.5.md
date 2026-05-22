---
project: agent-canvas
spec: v0.5
status: draft
created: 2026-05-22
author: cto
implements: AWS Interaction Protocol Spec v1 (ADR-003)
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
| `get_user_messages` shape | `{messages:[{id,session_id,path,note,action_verb,created_at}]}` (v0.4 wrap) | `{messages:[{interaction_id, payload:{§4}, ts}]}` (protocol §5) |
| `get_comments` shape | `{comments:[…sidecar…]}` (v0.4 wrap) | unchanged shape; feeds `document-review` responses |
| Rich UI | agent hand-rolls HTML (clipboard return) | Canvas renders typed classes from the envelope |
| Receipts | none (Slice 9 planned) | Delivered→Read→Responded keyed by `interaction_id` (folded in here) |

The v0.4 array→record wrap (commit `a7d9c44`) is the minimal unblock and stays; v0.5 evolves
the **per-item shape** to the protocol envelope.

## Target architecture

### Data model (backend)
New `interactions` table (mirrors the protocol's request+response, additive migration):

```
interaction_id TEXT PRIMARY KEY,   -- correlates request ↔ response
session_id     TEXT NOT NULL,      -- dispatching agent session
class          TEXT NOT NULL,      -- decision-set | document-review | approval-gate | visual-artifact
title          TEXT,
artifact_path  TEXT,               -- optional doc/HTML to render
trace_id       TEXT,               -- handoff-event boundary link
request_json   TEXT NOT NULL,      -- the §3 envelope as received
status         TEXT NOT NULL,      -- pending | submitted | draft | dismissed
response_json  TEXT,               -- the §4 structured payload on submit
created_at     INTEGER NOT NULL,
responded_at   INTEGER,
read_at        INTEGER             -- when the agent consumed it (Read receipt)
+ index on (session_id, status), (interaction_id)
```

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
- **Evolve `get_user_messages`** → `{messages:[{interaction_id, payload, ts}]}` where
  `payload` is the §4 response object for `status in (submitted, draft)` interactions for the
  caller's session (honor `since`). On read, set `read_at` (the Read receipt) inside the lock,
  emit `messages-changed`/`interaction-read` post-lock. Non-destructive (agents dedupe via
  `since`).
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
- **`visual-artifact`**: render the supplied HTML/image payload (reuse the sandboxed HTML
  viewer) + annotate → `comments[]`. This is the escape hatch for genuinely-custom rendering.

### Response builder
The v0.4 "Send to" dialog becomes the **response submitter**: it emits the §4 envelope
(`interaction_id` echoed, `artifact_path`, `status` = submitted|draft|dismissed,
`submitted_at` ISO-8601, plus the class-specific fields). No free-text-only path. "Dismiss"
sets `status:dismissed`; "Save draft" sets `status:draft`.

### Lifecycle, receipts, telemetry
- Emit Canvas-side lifecycle events: `interaction.dispatched`, `interaction.responded`,
  `interaction.dismissed`, and `interaction.read` (agent consumed). These drive the
  **Delivered → Read → Responded** receipt UI (folds in Slice 9), keyed by `interaction_id`.
- **Handoff-event emission is NOT Canvas's job** (open question — see below): Canvas exposes
  the lifecycle so the wm-agent spine / dispatching agent writes the boundary record
  (`human_in_loop:true`, `gate_type`, shared `trace_id`) per protocol §7. Canvas just carries
  and echoes `trace_id`.
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

## Open questions (capture, don't build)

- **Who writes the handoff-event boundary record** for a Canvas interaction — the dispatching
  agent, the wm-agent spine, or does Canvas emit a precursor event the spine consumes?
  (Protocol §7 says the interaction "emits" one; ownership of the *write* is unspecified.)
  Route to Forge.
- **`interaction_id` redundancy:** protocol §5 wraps as `{messages:[{interaction_id, payload,
  ts}]}` while §4's `payload` also carries `interaction_id`. Confirm canonical source with
  Forge before implementing the read shape.
- **Time format:** §5 uses epoch `ts`; §4 uses ISO-8601 `submitted_at`. Pick one for the
  stored `responded_at` / wire `ts`. Route to Forge.
- **`document-review` edit capture:** diff against the dispatched artifact version vs the
  on-disk version at submit (concurrent-edit window). Reuse the existing stat+hash guard.
- Whether `visual-artifact` accepts a file path, inline HTML, or both in the envelope.

## Acceptance (milestone)

A connected agent dispatches a `decision-set`; Canvas renders the form; the operator selects
options + notes and submits; the agent reads a structured `{responses:[{question_id,
selected:[keys], note}]}` correlated by `interaction_id` — with no clipboard and no
hand-authored HTML. `approval-gate`, `document-review`, `visual-artifact` likewise return
their §4 shapes. Canvas-down degrades to the agent's terminal fallback with no error. Gates:
`cargo test`, `tsc`, `vite build`, A22=0, A15=0.
