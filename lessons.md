---
project: vellum
updated: 2026-05-10
---

# Vellum — Lessons (hot buffer)

One line per insight, dated. Keep last 15. Promote cross-project patterns to KB at session wrap.

## 2026-05-10

- [pending] **Cross-model convergence on Critical findings is the strongest review signal.** Codex (implementation lens) and Claude -p (architectural lens) independently flagged the cold-state cold-state contradiction. When two reviewers from different model families converge on the same Critical, the fix is non-negotiable.
- [pending] **My own patches can introduce Criticals.** Cycle 1 patch on §Block identity created the cold-state contradiction. Caught only by cross-model external review. Lesson: internal cycles surface different issues than external; both are needed; verification cycle after every batch of patches is non-optional.
- [pending] **"Locked for scaffolding" doesn't mean review-complete.** A third review pass found 2 Criticals + many Highs that two prior paper reviews missed. Lock the spec for scope-control, not for review-stop.
- [pending] **Spec-vs-code ambiguity is the most common High finding.** Codex caught: prose said "serde ↔ zod via ts-rs" but ts-rs doesn't generate Zod; prose said "trust by handshake name" but handshake-name alone is spoofable; prose said "signed binaries" but signing is multiple operational tracks. Pattern: any time prose describes a concrete mechanism, the mechanism must be specified precisely OR the prose must explicitly name the gap.
- [pending] **Background CLI auth races are real.** Claude -p, Codex, and ChatMock all needed independent reauth — they don't share auth state. Plan for this when running multi-CLI external reviews.
- [pending] **Partition contracts are the right framing for parser correctness.** "Ordered, non-overlapping byte-span partition that reproduces the file" is a CI-testable invariant. Better than "byte-range preservation" as a vague property.
- [pending] **Auto-migration on rename was the right call.** Sidecar identity maps that don't follow `git mv` and Finder rename are useless. Source-hash matching + cache-wide scan on miss is cheap and solves the common case.
