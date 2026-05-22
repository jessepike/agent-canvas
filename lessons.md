---
project: vellum
updated: 2026-05-10
---

# Vellum — Lessons (hot buffer)

One line per insight, dated. Keep last 15. Promote cross-project patterns to KB at session wrap.

## 2026-05-10 (scaffold + Gate 30A + Gate 30B start)

- [pending] **Edit-source preference rule makes SerializeFromTree the rare path — stub it.** Most edits go through PreservedBytes or EditedBytes branches. Implementing SerializeFromTree fully on day 1 is YAGNI; stub returns `Err(SerializerUnimplemented)`, ship the common path.
- [pending] **Auto-migration on rename was the right call.** Sidecar identity maps that don't follow `git mv` and Finder rename are useless. Source-hash matching + cache-wide scan on miss is cheap and solves the common case.

## 2026-05-21 (v0.3.0 release wrap)

