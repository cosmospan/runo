# Full UI redesign mockups

Three directions for a Runo UI redesign, drawn like-for-like so they're easy to compare.
Everything here is **design only** — static HTML/CSS rendered to PNG with headless Chrome,
no app code changes.

Each direction covers the same five composed surfaces, in both **light** and **dark**:

| Surface            | What it shows                                                                         |
|--------------------|---------------------------------------------------------------------------------------|
| **Main window**    | Chrome, sidebar (Crew + grouped threads + Sessions), chat with a user message + image, agent reply with markdown, step group, Working indicator, From-thread chip, and composer |
| **Panels**         | Thread peek pane, menu bar panel, and Assistant panel laid out side by side           |
| **Arcade**         | The modal with one game (Spark) and the Game over card                                |
| **Settings**       | The Look page (so you can see every control: toggle, segmented, picker)               |
| **Translucent**    | Wallpaper visible behind glass window chrome and sidebar (ties into PR #300)          |

30 images total (3 × 5 × 2). All PNGs are 2x; the main windows are 1440×900.

The content is pulled from today's app — real thread titles ("Translucency across modals",
"Full UI redesign mockups", "Dock icon badge count", "Compact usage indicator"), real
branches, the real Working step strings from `working.rs`, and the real Settings tab
layout from `settings_ui.rs`.

## How to view and render

- **View a single surface in a browser:** open `a.html?s=main&t=dark` (or `b.html` / `c.html`).
  Supported `s=`: `main`, `panels`, `arcade`, `settings`, `translucent`. Supported `t=`:
  `light`, `dark`.
- **Render all PNGs:** `./render.sh`. For a subset: `./render.sh c` or `./render.sh b main`.

The share folder has three files only:
- `shared/base.css` — reset, type, scrollbars off, the wallpaper + "image attachment" fakes
- `shared/data.js`  — all the content strings (threads, chat, menubar, arcade, settings, assistant)
- `shared/icons.js` — SVG icon set + agent marks + dot/ring/spinner/diff helpers

Each `a.html` / `b.html` / `c.html` is self-contained: it carries its own token palette
and its own version of every surface's CSS, then uses the shared data to fill them in.

## The three directions, side by side

| | Light | Dark |
|---|---|---|
| **A · Linear-like**, main window              | ![](out/a-main-light.png) | ![](out/a-main-dark.png) |
| **A · Linear-like**, panels                    | ![](out/a-panels-light.png) | ![](out/a-panels-dark.png) |
| **A · Linear-like**, Arcade                    | ![](out/a-arcade-light.png) | ![](out/a-arcade-dark.png) |
| **A · Linear-like**, Settings                  | ![](out/a-settings-light.png) | ![](out/a-settings-dark.png) |
| **A · Linear-like**, translucent               | ![](out/a-translucent-light.png) | ![](out/a-translucent-dark.png) |
| **B · Raycast/Arc-like**, main window          | ![](out/b-main-light.png) | ![](out/b-main-dark.png) |
| **B · Raycast/Arc-like**, panels               | ![](out/b-panels-light.png) | ![](out/b-panels-dark.png) |
| **B · Raycast/Arc-like**, Arcade               | ![](out/b-arcade-light.png) | ![](out/b-arcade-dark.png) |
| **B · Raycast/Arc-like**, Settings             | ![](out/b-settings-light.png) | ![](out/b-settings-dark.png) |
| **B · Raycast/Arc-like**, translucent          | ![](out/b-translucent-light.png) | ![](out/b-translucent-dark.png) |
| **C · Minimalist (Rams)**, main window         | ![](out/c-main-light.png) | ![](out/c-main-dark.png) |
| **C · Minimalist (Rams)**, panels              | ![](out/c-panels-light.png) | ![](out/c-panels-dark.png) |
| **C · Minimalist (Rams)**, Arcade              | ![](out/c-arcade-light.png) | ![](out/c-arcade-dark.png) |
| **C · Minimalist (Rams)**, Settings            | ![](out/c-settings-light.png) | ![](out/c-settings-dark.png) |
| **C · Minimalist (Rams)**, translucent         | ![](out/c-translucent-light.png) | ![](out/c-translucent-dark.png) |

---

## A · Linear-like

![](out/a-main-light.png)

Tight rows (26 px threads, 32 px sessions), muted gray surfaces, 1 px hairlines, no card
chrome, small accent in indigo. The Crew block uses a soft wash with a thin edge rather
than a tinted card. The composer is a plain rounded rectangle at the bottom; the status
bar is 24 px with tabular numbers. Chat is left-aligned, no bubbles.

**What works:**

- You see *more* at once. The sidebar fits all four groups plus five sessions without
  scrolling, and chat shows a full agent reply + step group + Working on the same screen.
- Keeps the current app's palette essentially intact: the "runo" light theme and
  "tokyo-night" dark theme both drop in cleanly.
- Status bar + hairlines read as "serious tool", a good fit for Runo's actual target
  user (people running multiple agents at once).
- Keyboard-first affordances (⌘K hint in search, ⌘⏎ on Send) feel natural in this density.

**Trade-offs / what breaks:**

- The gain in density costs some warmth. The Working indicator in particular is subtler
  than today's shimmer and the amber "needs you" pill is small — people glancing at the
  menu bar need to really *look*.
- Playful surfaces (Arcade, Assistant panel) feel almost begrudging in this treatment —
  Arcade in Variant A reads as "serious game", not a reward.
- With everything so flat, hover and selection states have to carry a lot of weight.
  You'll want to test accessibility of the focus ring (`accent-wash-edge`) against the
  main sidebar background.

## B · Raycast/Arc-like

![](out/b-main-light.png)

Rounded cards everywhere (12–18 px radius), a lilac-tinted canvas with soft radial
gradients, Crew sits in a gradient card, chat messages are elevated cards, and the
composer floats with its own ring. Accent is a vibrant purple + raspberry pair; the
Working indicator is a filled pill, not a dot.

**What works:**

- The whole thing *reads* differently from any other coding tool, which may be the
  single biggest branding win here — Runo's "friendly serious" voice gets a visual match.
- Elevated cards make hover + selection effortless: shadow + 1 px of lift do the work
  with no extra chrome. The Crew gradient card is unmistakable at a glance.
- Arcade and the Assistant panel finally feel like *rewards*, not escape hatches. The
  Game-over card with the gradient score is a 10× nicer win.
- The translucent scenario shines here — the tinted canvas already blends with wallpapers,
  so glass isn't a "mode" the UI has to compensate for.

**Trade-offs / what breaks:**

- Density drops. The sidebar fits roughly the same information but needs ~20% more
  vertical room; on a 13" laptop a long "Working" list will fold earlier.
- The strong accent + gradients only read right on themes that respect them — teams
  that love Tokyo Night or Catppuccin may want to tone it down (so you'll either need a
  "muted B" theme, or make the gradient a token).
- Shadow + blur used liberally — performance on an idle M-series Mac is fine, but
  low-end iGPU + external 4K monitor is the usual Runo testbed, worth measuring.
- Risk: this reads as "AI chat app" more than "developer multiplexer". That's the
  thing to watch during user testing.

## C · Minimalist (Dieter Rams)

![](out/c-main-light.png)

Mostly white or near-black, one accent in Rams-ish orange-red, hairlines only, no card
chrome, large type sizes (20 px thread title, 32 px settings heading), uppercase labels
with letter-spacing. Hierarchy from type and spacing alone; the selected thread grows a
3 px accent bar; "Needs you" is a thin uppercase label.

**What works:**

- The best fit for "the quiet tool that lets your work speak". Chat is uncluttered
  reading, the Working indicator is a 2 ch label + a square dot — information without
  decoration.
- Settings becomes genuinely elegant: `h1` is big and quiet, controls are underline-only,
  and segmented controls lose their boxes. The Look page actually looks *good* at rest.
- The accent carries the whole brand — one color across every surface, used sparingly.
  Easy to theme for teams that don't want purple.
- Translucent reads well because there's almost no surface for the blur to muddy.

**Trade-offs / what breaks:**

- Needs great typography to not feel bare — any system font fallback (looking at you,
  Linux) will erode the hierarchy. SF Pro Display must be guaranteed.
- Loses the ability to color-code status cheaply. Working, Needs you and Watching CI all
  share a very similar neutral treatment; most tone separation moves to shape/letter
  (dot vs square vs dash) which may hurt accessibility.
- Some surfaces feel *underdressed* — the Arcade game-over card with no gradient on the
  score is handsome but less fun than today's shimmer-tinted treatment.
- Risk: people may read "quiet" as "unopinionated" and want more affordances. This is
  the direction most likely to need a second pass for discoverability (where's the
  "Attach" button? Harder to find without a chip).

---

## Recommendation

**I'd start with B (Raycast/Arc-like), and borrow one specific thing from A.**

Why B:

- Runo's differentiator over Linear/Zed/VS Code is not density, it's that a *crew of
  agents* runs together. The gradient Crew card + card-based chat make that physically
  visible in a way neither A nor C can match. The Assistant panel and Arcade stop being
  afterthoughts.
- It handles the translucent story (PR #300) with the lowest additional effort.
- B gives you a cheaper upgrade path from today's look: today's sidebar is already a
  card with a soft background, so you're not rewriting every surface at once.

What to borrow from A:

- **The status bar** (24 px, tabular numerics, hairline). B's "pill" status bar is cute
  but noisy; A's wins. It also gives the menubar app and the main window the same bottom
  trim, which matters.
- **The dense sidebar collapse rule** — in B, when the window is narrow, degrade to A's
  26 px rows instead of folding to icons. (You get density back for free when you need it.)

What I'd *not* ship from B as-is:

- The chrome pill bar is pretty but costs real estate. Try a flatter version (no shadow,
  no rounded ring) and A/B it.
- The accent is on the edge of too saturated for dark mode. Pin it to Tokyo Night's
  `#a383ff` and dial back the raspberry pair.

If the team's identity is pointing toward "quiet tool" (C) that's a defensible pick too
— but you'll pay for it in copywriting work (every empty state has to be hand-tuned to
not feel bare).

---

## Prior art in the repo

- The Crew-in-sidebar variant A from PR #238 (and the follow-ups #254, #257) is the
  current baseline. All three directions here inherit its grouping ("Needs you",
  "Working", "Watching CI", "Waiting", "N done" fold).
- The compact usage indicator (PR #252) shows up in every chrome.
- The borderless sections (PR #257) are already visible in A and C. B deliberately
  re-introduces card edges because it's using them for hierarchy.
