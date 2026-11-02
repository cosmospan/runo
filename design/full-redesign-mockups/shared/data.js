// Realistic Runo content for the three-direction mockup set.
// Pulled from the current Native/runo-app/src (crew_view.rs, menubar_panel.rs,
// askbar.rs, arcade.rs, settings_ui.rs, assistant.rs, working.rs).

const PROJECT = "runo-lab";

const AGENTS = {
  claude:   { name: "Claude Code", mark: "✳", color: "#d9633f" },
  codex:    { name: "Codex",       mark: "◆", color: "#1767e8" },
  opencode: { name: "OpenCode",    mark: ">_", color: "#258a5b" },
  gemini:   { name: "Gemini",      mark: "✦", color: "#4f7cf7" },
};

const THREADS = [
  { id: "trans", st: "needs", title: "Translucency across modals", ag: "claude",
    br: "runo/translucent-modals", age: "9m",
    ask: "One blur across all modals, or per-modal?" },
  { id: "redesign", st: "working", title: "Full UI redesign mockups", ag: "claude",
    br: "runo/full-ui-redesign-mockups", age: "8m",
    size: "480 MB", diff: [0, 0], sel: true, started: "9:14 PM" },
  { id: "dock", st: "working", title: "Dock icon badge count", ag: "codex",
    br: "runo/dock-icon-badge", age: "4m",
    size: "720 MB", unread: true, diff: [26, 11] },
  { id: "usage", st: "watching", title: "Compact usage indicator", ag: "claude",
    br: "runo/compact-usage-indicator", age: "22m",
    pr: 252, size: "900 MB", diff: [138, 54], why: "CI running" },
  { id: "border", st: "waiting", title: "Borderless sidebar sections", ag: "claude",
    br: "runo/borderless-sidebar-sections", age: "1h",
    pr: 257, size: "1.1 GB", diff: [84, 31], why: "Waiting for review" },
];
const DONE = [
  { title: "Stunning working indicator", pr: 247 },
  { title: "Thread message chip fix",    pr: 245 },
  { title: "Low priority background checks", pr: 244 },
  { title: "Refine Arcade: pro + fun",   pr: 243 },
  { title: "Auto-clean worktree builds", pr: 246 },
];
const DONE_N = 11;

// The person's own sessions, drawn below Crew in the sidebar.
const OWN_SESSIONS = [
  { title: "Add a guide page for docs", ag: "claude", sub: "runo/docs-guide-page", when: "8:12 PM", diff: [86, 4] },
  { title: "Bump gpui and fix warnings", ag: "claude", sub: "runo/gpui-bump", when: "Oct 24", diff: [31, 27] },
  { title: "Update the docs", ag: "codex", sub: "Codex · gpt-5.5", when: "Oct 22" },
];

// Chat shown in the main window for the selected thread "Full UI redesign mockups".
// User msg + attached image + agent reply with markdown + step group + working indicator.
const CHAT = {
  fromChip: { thread: "Translucency across modals", status: "needs you" },
  turns: [
    { who: "user", text: "Can you make the modal blur match the window chrome? Here's the mismatch — the glass behind Settings looks lighter than the main sidebar.",
      image: true },
    { who: "agent", ag: "claude",
      heading: "Fixing the modal blur",
      body: [
        "The problem is that `tint()` in `settings.rs` reads `glass_on()`, but the modal canvas calls `rgb()` directly. Three small changes:",
        { kind: "list", items: [
          "Have the modal canvas use `tint(c_canvas())` so the wallpaper shows through",
          "Keep the border crisp with `c_subtle()` at 1 px",
          "Pin the shadow at 20 % so it reads the same against any wallpaper"
        ]},
        "I'll check the alerts stack doesn't double-blur."
      ],
      steps: [
        { label: "Read Native/runo-app/src/alerts.rs",  state: "done" },
        { label: "Read Native/runo-app/src/settings.rs", state: "done" },
        { label: "Edit Native/runo-app/src/alerts.rs",   state: "running" },
      ],
      working: { label: "Editing", secs: 6 }
    },
  ],
  composer: {
    placeholder: "Describe a task to start, or search sessions and history\u2026",
    mode: "Direct",
    modes: ["Direct", "Crew"],
    chips: ["Attach", "@thread"],
  }
};

// Menu bar panel contents (Activity tab).
const MENUBAR = {
  tabs: ["Activity", "Ask Runo"],
  active: "Activity",
  tiles: [
    { label: "Working",   n: 2, tone: "blue" },
    { label: "Needs you", n: 1, tone: "amber" },
    { label: "To review", n: 1, tone: "green" },
  ],
  sections: [
    { title: "Needs you", rows: [
      { kind: "approval", project: "runo-lab", thread: "Translucency across modals",
        agent: "claude",
        ask: "May I run `cargo check`?",
        code: "cargo check -p runo-app",
        actions: ["Allow", "Always", "Deny"] },
    ]},
    { title: "Working", rows: [
      { kind: "session", project: "runo-lab", thread: "Full UI redesign mockups",
        agent: "claude", step: "Editing alerts.rs", t: "8m" },
      { kind: "session", project: "runo-lab", thread: "Dock icon badge count",
        agent: "codex", step: "Running cargo test", t: "4m" },
    ]},
    { title: "Watching CI", rows: [
      { kind: "ci", project: "runo-lab", thread: "Compact usage indicator",
        pr: 252, phase: "clippy", t: "22m" },
    ]},
  ],
  input: "Ask, or tell Runo what to do\u2026",
};

// Thread peek pane content for "Translucency across modals".
const PEEK = {
  title: "Translucency across modals",
  agent: "claude",
  branch: "runo/translucent-modals",
  status: "needs you",
  decision: "Needs your call on scope",
  turns: [
    { who: "crew", text: "Should the blur be one radius across every modal, or per-modal so Arcade can stay crisper?" },
    { who: "agent", ag: "claude", text: "I can go either way. The one-blur path is 20 lines; the per-modal path adds a `glass_level` to `AlertStack`. Pick one." },
  ],
  question: "One blur across all modals, or per-modal?",
};

// Arcade modal content (one game + game-over card).
const ARCADE = {
  game: "Spark",
  tagline: "Fly through gates",
  hint: "Space or click to lift",
  score: 1284,
  best: 2041,
  gameOver: { score: 1284, best: 2041, retry: "Space to retry", leave: "Esc to leave" },
  others: ["Commits", "Merge", "Stack"],
};

// Settings page shown on the Look tab.
const SETTINGS = {
  nav: ["Overview", "General", "Look", "Agents", "Connections", "Safety", "Keys"],
  active: "Look",
  rows: [
    { title: "Theme", desc: "The window's palette. Light and dark follow the system by default.",
      control: "picker", value: "runo · Follow system", options: ["runo", "catppuccin-latte", "tokyo-night", "night-owl"] },
    { title: "Glass", desc: "Let the wallpaper show through sidebars and modals.",
      control: "toggle", value: "On" },
    { title: "Font",  desc: "The text face used everywhere but code.",
      control: "picker", value: "SF Pro Text", options: ["SF Pro Text", "Inter", "System"] },
    { title: "Content width", desc: "How wide chat and settings pages are allowed to grow.",
      control: "segmented", value: "Comfortable", options: ["Narrow", "Comfortable", "Wide"] },
    { title: "Reduce motion", desc: "Replace the working indicator's shimmer with a static dot.",
      control: "toggle", value: "Off" },
  ],
};

// Assistant corner mark + panel.
const ASSISTANT = {
  mood: "idle",      // idle | listening | thinking | needs
  unread: 2,
  heading: "Runo",
  sub: "Ask how to do anything",
  starters: [
    "How do I race Claude Code and Codex on one task?",
    "Set up a playbook for my branch cleanup",
    "What does 'Needs you' mean?",
    "Show me the keybindings",
  ],
  recents: [
    { q: "How does Crew pick which agent runs?", t: "yesterday" },
    { q: "Where are my worktrees?",              t: "2 days ago" },
  ],
};

// Translucent scenario (wallpaper visible, glass surfaces over it).
const WALLPAPER = {
  // Soft dawn gradient we can draw directly; no external image needed.
  gradient: "linear-gradient(135deg, #ffb37a 0%, #ff80b2 30%, #8e63ff 70%, #2b47c7 100%)",
  gradientDark: "linear-gradient(135deg, #1a0f3a 0%, #2d1b55 35%, #3a2a66 70%, #0d1430 100%)",
};

window.RUNO = {
  PROJECT, AGENTS, THREADS, DONE, DONE_N, OWN_SESSIONS, CHAT,
  MENUBAR, PEEK, ARCADE, SETTINGS, ASSISTANT, WALLPAPER,
};
