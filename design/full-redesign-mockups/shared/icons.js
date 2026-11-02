// Minimal SVG icon set. Every icon takes a stroke colour and an optional size.
// Keeps the three directions consistent on glyph choice so comparisons are fair.

const ICONS = {
  search: (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6"><circle cx="7" cy="7" r="4.6"/><path d="M10.6 10.6 14 14" stroke-linecap="round"/></svg>`,
  chevD:  (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6l4 4 4-4"/></svg>`,
  chevR:  (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg>`,
  chevL:  (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M10 4l-4 4 4 4"/></svg>`,
  folder: (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.4" stroke-linejoin="round"><path d="M2 4.5c0-.8.6-1.5 1.5-1.5h3l1.5 1.5h4.5c.8 0 1.5.7 1.5 1.5v5.5c0 .8-.7 1.5-1.5 1.5h-9C2.7 13 2 12.3 2 11.5z"/></svg>`,
  plus:   (c, s=13) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10"/></svg>`,
  crew:   (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.5" stroke-linecap="round"><circle cx="6" cy="5.5" r="2.3"/><path d="M1.8 13c.4-2.3 2.1-3.6 4.2-3.6s3.8 1.3 4.2 3.6"/><circle cx="11.4" cy="5.8" r="1.8"/><path d="M11.2 9.4c1.7 0 2.8 1.1 3.1 3"/></svg>`,
  check:  (c, s=12) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8.5l3.2 3.2L13 4.8"/></svg>`,
  branch: (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"><circle cx="4.5" cy="3.5" r="1.6"/><circle cx="4.5" cy="12.5" r="1.6"/><circle cx="11.5" cy="5" r="1.6"/><path d="M4.5 5.1v5.8M11.5 6.6c0 3-4.2 2.4-6.6 4.6"/></svg>`,
  pr:     (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"><circle cx="4" cy="3.5" r="1.6"/><circle cx="4" cy="12.5" r="1.6"/><circle cx="12" cy="12.5" r="1.6"/><path d="M4 5.1v5.8M12 10.9V6.5c0-1.2-.8-2-2-2H7.5M9 3 7.4 4.5 9 6"/></svg>`,
  disk:   (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.5"><rect x="2" y="9" width="12" height="4.5" rx="1.3"/><path d="M3 9l1.6-5h6.8L13 9"/><circle cx="11" cy="11.25" r=".6" fill="${c}"/></svg>`,
  clock:  (c, s=11) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"><circle cx="8" cy="8" r="6"/><path d="M8 4.8V8l2.2 1.4"/></svg>`,
  attach: (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M11.5 7 7.1 11.4a2.1 2.1 0 1 1-3-3L8.5 4a3.4 3.4 0 0 1 4.8 4.8L8.9 13.2"/></svg>`,
  send:   (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M8 13V3M4 7l4-4 4 4"/></svg>`,
  settings: (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.5"><circle cx="8" cy="8" r="2.1"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/></svg>`,
  maxi:   (c, s=12) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7V3h4M13 9v4H9M9 3h4v4M3 9v4h4"/></svg>`,
  more:   (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="${c}"><circle cx="3.5" cy="8" r="1.1"/><circle cx="8"   cy="8" r="1.1"/><circle cx="12.5" cy="8" r="1.1"/></svg>`,
  spark:  (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="${c}"><path d="M8 1l1.6 4.4L14 7l-4.4 1.6L8 13l-1.6-4.4L2 7l4.4-1.6z"/></svg>`,
  wifi:   (c, s=12) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.5" stroke-linecap="round"><path d="M3 7a7.5 7.5 0 0 1 10 0M5 9.5a4.5 4.5 0 0 1 6 0"/><circle cx="8" cy="12" r="1" fill="${c}"/></svg>`,
  arcade: (c, s=14) => `<svg width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="${c}" stroke-width="1.5" stroke-linejoin="round"><rect x="2" y="3" width="12" height="8.5" rx="1.5"/><path d="M4.5 7.5h2M5.5 6.5v2M10 8l1 1M10 7l-1-1"/><path d="M4 13h8"/></svg>`,
  logo:   (c, s=20) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="${c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M8 15V9h5a2.5 2.5 0 0 1 0 5h-4l5 4"/></svg>`,
};

const DOT = c => `<i style="width:7px;height:7px;border-radius:50%;background:${c};display:inline-block"></i>`;
const RING = c => `<i style="width:8px;height:8px;border-radius:50%;border:1.6px solid ${c};display:inline-block"></i>`;
const SPIN = c => `<svg width="12" height="12" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6" stroke="${c}" stroke-opacity=".22" stroke-width="2.4"/><path d="M8 2a6 6 0 0 1 6 6" stroke="${c}" stroke-width="2.4" stroke-linecap="round"/></svg>`;
const DIFF = (a, d, aC = "#16a34a", dC = "#dc2626") => `<span style="font-family:'SF Mono',Menlo,monospace;font-size:11px"><span style="color:${aC}">+${a}</span> <span style="color:${dC}">-${d}</span></span>`;

// A tiny agent "mark" tile. The real app renders these per agent; we fake it with the glyph.
const AGENT_MARK = (agKey, s = 14) => {
  const a = window.RUNO.AGENTS[agKey];
  return `<span class="ag-mark" style="width:${s + 4}px;height:${s + 4}px;background:${a.color}22;color:${a.color};
    display:inline-flex;align-items:center;justify-content:center;border-radius:5px;
    font-weight:700;font-size:${Math.floor(s * 0.8)}px;line-height:1">${a.mark}</span>`;
};

window.ICONS = ICONS;
window.DOT = DOT;
window.RING = RING;
window.SPIN = SPIN;
window.DIFF = DIFF;
window.AGENT_MARK = AGENT_MARK;
