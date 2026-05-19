# AgentCanvas — Visual System v0.1

Captured from Round 1 prototypes (the other agent's design choices) on 2026-05-19.
Round 2 and beyond should preserve this system unless a deliberate decision moves it.

## Typography

```css
--font-ui:      'Funnel Sans', -apple-system, BlinkMacSystemFont, system-ui, sans-serif;
--font-display: 'Funnel Display', 'Funnel Sans', sans-serif;
--font-serif:   'Newsreader', 'Iowan Old Style', Georgia, serif;
--font-mono:    'JetBrains Mono', ui-monospace, 'SF Mono', monospace;
```

**Role assignments:**
- **Funnel Sans** — all chrome (sidebar, toolbar, buttons, file rows, labels)
- **Funnel Display** — large titles, document H1s, hero-weight UI
- **Newsreader** — prose body (rendered Markdown), prose-persona agent badges (CTO, CPO, CISO), serif emphasis
- **JetBrains Mono** — code blocks, backbone tags (claude, codex), inline code, monospaced data

**Letter-spacing convention:** tight (-0.02em to -0.005em) for display and large UI; positive (0.06em–0.08em) for small-caps section labels (INBOX, PINNED, PROJECTS, ACTIONS).

## Color System

### Window & Surface

```css
--bg-desktop:       #C8C5BC   /* warm taupe (behind the window) */
--bg-window:        #FBFAF6   /* warm off-white — primary surface */
--bg-sidebar:       #EFEDE6   /* slightly cooler off-white */
--bg-titlebar-top:  #F2F0E8
--bg-titlebar-bot:  #E7E4DA
--bg-doc:           #FFFFFF   /* document canvas (pure white) */
```

### Borders & Hover

```css
--border-soft:    #E8E5D9
--border:         #DDD9CC
--border-strong:  #C6C1B0
--hover-bg:       #E5E2D8
--selected-bg:    #FFFFFF
```

### Text

```css
--text:            #1B1A14   /* near-black warm */
--text-secondary:  #6E6A5E
--text-tertiary:   #A09B8B
--text-faded:      #BFBAAB
```

### Accent — Ink Blue

```css
--accent:           #1F5BD8   /* primary ink-blue */
--accent-deep:      #173F95   /* hover / pressed */
--accent-soft:      #E5EBFA   /* selected backgrounds, subtle fills */
--accent-soft-deep: #D5E0F4
```

### Pin (gold)

```css
--pin:       #C19433
--pin-soft:  #F6ECCF
```

### Pending Edit (warm orange)

```css
--pending:        #C2691E
--pending-deep:   #8E4912
--pending-bg:     #FCEED9   /* banner background */
--pending-border: #E8C696
--pending-row-bg: #FCF1DC   /* row tint for files with pending edits */
```

### Diff (subdued, organic)

```css
--diff-add-bg:           #DEEED2
--diff-add-text:         #2A5A1F
--diff-add-strong:       #5A8C36
--diff-add-block:        #ECF5DE
--diff-add-block-border: #B9D49A
--diff-rem-bg:           #F2DCD9
--diff-rem-text:         #7A2424
--diff-rem-strong:       #B8453A
```

### Drop Target

```css
--drop-target-bg:     #DCE7FB
--drop-target-border: var(--accent)
```

## Shadows

```css
--shadow-window:      0 28px 70px rgba(20,18,12,0.30), 0 10px 24px rgba(20,18,12,0.16);
--shadow-toolbar-btn: 0 1px 0 rgba(255,255,255,0.6) inset, 0 1px 2px rgba(0,0,0,0.04);
```

Window shadow is generous and warm (RGBA dark warm, not pure black) — sits the app on the desktop without feeling stark.

## Aesthetic Principles

1. **Warm off-white over cool gray.** The chrome is paper-warm (#FBFAF6, #EFEDE6) rather than the cool blue-gray of system defaults. Pairs with the warm-ink-blue accent.
2. **Ink-blue is the single accent.** No purple, no teal, no rainbow categorization. Pin gold and pending orange are functional, not decorative.
3. **Borders subtle, never harsh.** All borders sit on the warm-cream axis (#DDD9CC etc.) — they delineate without shouting.
4. **Typography hierarchy by family, not weight.** Funnel Sans for chrome, Newsreader for prose, JetBrains Mono for code. The font switch *is* the hierarchy.
5. **Negative letter-spacing on large type, positive on small-caps labels.** Small-caps tracking (0.06–0.08em) is what makes section headers feel intentional.
6. **Diff colors are subdued and organic.** Not GitHub-saturated. The diff reads as part of prose, not a code-review tool grafted in.
7. **Pulsing dot for "thinking" states.** Used on active agent sessions. Subtle ink-blue circle, not red/green.

## Native-build promotion notes

When this moves from HTML prototype to Tauri or native Swift:

- **Sidebar should use `NSVisualEffectView` material** (vibrancy). HTML mocks this with solid warm cream; native should be translucent over the desktop wallpaper.
- **Title bar same — vibrancy over solid.**
- **Window shadow becomes native macOS window shadow** (handled by AppKit).
- **System font fallback** is already wired in the CSS chains — if Funnel Sans / Newsreader fonts fail to load, the app falls back to SF Pro / Iowan Old Style cleanly.

## Round-2 commitments

- Apply this same system to E (agent panel), F (project-detail), I (command palette), K (keyboard-first).
- Do not introduce new colors unless functionally necessary. If a new state needs a color, propose it as an addition here first.
- Persona badge typography: italic Newsreader serif for prose personas (CTO, CPO, CISO); JetBrains Mono for code-flavored (codex); custom personas get a distinct treatment to be designed in E.
