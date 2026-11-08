// Squad's page helpers, run in the page the bot drives. `A` holds the call's arguments
// and `OP` names the call. The outline reads a page as a short list of lines: headings,
// text and numbered controls, so a bot can see and use a page without a screenshot.
// Numbers stay with their element for as long as the page is open.
const S = (window.__sq = window.__sq || { n: 0, ids: new WeakMap(), els: new Map() });
const CONTROLS =
  'a[href],button,input:not([type=hidden]),select,textarea,summary,[role],[contenteditable=""],[contenteditable=true]';
const ROLES = new Set([
  'button', 'link', 'checkbox', 'radio', 'tab', 'menuitem', 'menuitemcheckbox',
  'menuitemradio', 'switch', 'option', 'combobox', 'textbox', 'searchbox', 'slider',
  'spinbutton', 'treeitem',
]);
const SKIP = new Set([
  'SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'HEAD', 'META', 'LINK', 'IFRAME', 'svg',
  'CANVAS', 'VIDEO', 'AUDIO', 'IMG', 'PICTURE', 'BR', 'HR',
]);
const clip = (s, n) => {
  s = String(s || '').replace(/\s+/g, ' ').trim();
  return s.length > n ? s.slice(0, n - 1) + '…' : s;
};
const control = (el) => {
  const t = el.tagName;
  if (t === 'A') return el.hasAttribute('href');
  if (t === 'BUTTON' || t === 'SELECT' || t === 'TEXTAREA' || t === 'SUMMARY') return true;
  if (t === 'INPUT') return el.type !== 'hidden';
  const role = el.getAttribute('role');
  if (role && ROLES.has(role)) return true;
  return el.isContentEditable && !(el.parentElement && el.parentElement.isContentEditable);
};
const nameOf = (el) => {
  const aria = el.getAttribute('aria-label');
  if (aria && aria.trim()) return aria;
  const by = el.getAttribute('aria-labelledby');
  if (by) {
    const t = by.split(/\s+/).map((id) => (document.getElementById(id) || {}).innerText || '').join(' ');
    if (t.trim()) return t;
  }
  if (el.tagName === 'INPUT' && ['submit', 'button', 'reset'].includes(el.type)) return el.value;
  if (el.labels && el.labels.length && el.labels[0].innerText.trim()) return el.labels[0].innerText;
  if (!['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)) {
    const t = el.innerText;
    if (t && t.trim()) return t;
  }
  const img = el.querySelector && el.querySelector('img[alt]');
  return (
    el.getAttribute('placeholder') || el.getAttribute('title') || el.getAttribute('alt') ||
    (img && img.alt) || el.getAttribute('name') || el.id || ''
  );
};
const kindOf = (el) => {
  const role = el.getAttribute('role');
  if (role && ROLES.has(role)) return role;
  const t = el.tagName;
  if (t === 'A') return 'link';
  if (t === 'INPUT') {
    const ty = el.type;
    if (['submit', 'button', 'reset', 'image'].includes(ty)) return 'button';
    if (ty === 'checkbox' || ty === 'radio') return ty;
    return ty === 'text' ? 'textbox' : ty + ' field';
  }
  if (t === 'TEXTAREA') return 'textbox';
  if (t === 'SELECT') return 'select';
  if (t === 'SUMMARY') return 'summary';
  if (el.isContentEditable) return 'editor';
  return t.toLowerCase();
};
const shown = (el) => {
  const r = el.getBoundingClientRect();
  if (r.width <= 0 || r.height <= 0) return false;
  const cs = getComputedStyle(el);
  return cs.visibility !== 'hidden' && cs.opacity !== '0';
};
const refOf = (el) => {
  let r = S.ids.get(el);
  if (!r) {
    r = String(++S.n);
    S.ids.set(el, r);
  }
  S.els.set(r, new WeakRef(el));
  return r;
};
const describe = (el) => {
  const kind = kindOf(el);
  let line = `[${refOf(el)}] ${kind} "${clip(nameOf(el), 80)}"`;
  const t = el.tagName;
  if (t === 'A') {
    const href = el.getAttribute('href') || '';
    if (href && !href.startsWith('#') && !href.startsWith('javascript:')) {
      let h = href;
      try {
        const u = new URL(href, location.href);
        h = u.origin === location.origin ? u.pathname + u.search + u.hash : u.href;
      } catch (e) {}
      line += ' → ' + clip(h, 70);
    }
  } else if (t === 'SELECT') {
    const sel = el.selectedOptions[0];
    line += ` = "${clip(sel ? sel.text : '', 40)}"`;
    const opts = [...el.options].slice(0, 12).map((o) => clip(o.text, 30));
    line += ` (options: ${opts.join(' | ')}${el.options.length > 12 ? ' | …' : ''})`;
  } else if (kind === 'checkbox' || kind === 'radio' || kind === 'switch') {
    const on = el.checked !== undefined ? el.checked : el.getAttribute('aria-checked') === 'true';
    line += on ? ' (checked)' : ' (unchecked)';
  } else if ('value' in el && (t === 'INPUT' || t === 'TEXTAREA')) {
    if (el.type === 'password') line += el.value ? ' = "•••"' : ' (empty)';
    else line += el.value ? ` = "${clip(el.value, 60)}"` : ' (empty)';
  }
  if (el.disabled || el.getAttribute('aria-disabled') === 'true') line += ' (disabled)';
  if (el.getAttribute('aria-expanded')) line += el.getAttribute('aria-expanded') === 'true' ? ' (open)' : ' (closed)';
  if (el.required) line += ' (required)';
  let bad = false;
  try { bad = !!el.validationMessage && el.matches(':user-invalid'); } catch (e) {}
  if (bad) line += ` (invalid: ${clip(el.validationMessage, 60)})`;
  return line;
};
const blockish = (el) => {
  const d = getComputedStyle(el).display;
  return !d.startsWith('inline') && d !== 'contents';
};
const outline = () => {
  const lines = [];
  const visit = (el) => {
    if (SKIP.has(el.tagName) || el.getAttribute('aria-hidden') === 'true') return;
    // A field's label is already its name in the field's line.
    if (el.tagName === 'LABEL' && el.control && !el.querySelector(CONTROLS)) return;
    const cs = getComputedStyle(el);
    if (cs.display === 'none') return;
    if (control(el)) {
      if (shown(el)) lines.push(describe(el));
      return;
    }
    const h = /^H([1-6])$/.exec(el.tagName);
    if (h) {
      const t = clip(el.innerText, 120);
      if (t && shown(el)) lines.push('#'.repeat(+h[1]) + ' ' + t);
      if (!el.querySelector(CONTROLS)) return;
    }
    // A run of words with no controls or blocks inside reads as one line.
    const kids = [...el.children];
    if (!h && !el.querySelector(CONTROLS) && !kids.some(blockish)) {
      const t = cs.visibility === 'hidden' ? '' : clip(el.innerText, 200);
      if (t) lines.push((el.tagName === 'LI' ? '- ' : '') + t);
      return;
    }
    if (!h) {
      for (const n of el.childNodes) {
        if (n.nodeType === 3) {
          const t = clip(n.textContent, 200);
          if (t && cs.visibility !== 'hidden') lines.push(t);
        } else if (n.nodeType === 1) visit(n);
      }
    } else for (const k of kids) visit(k);
    if (el.shadowRoot) for (const k of el.shadowRoot.children) visit(k);
  };
  if (document.body) visit(document.body);
  const out = [];
  for (const l of lines) if (out[out.length - 1] !== l) out.push(l);
  const open = document.querySelector('dialog[open],[role=dialog],[role=alertdialog],[aria-modal=true]');
  return {
    title: document.title,
    url: location.href,
    height: Math.round(document.documentElement.scrollHeight),
    scrolled: Math.round(scrollY),
    dialog: open && shown(open) ? clip(nameOf(open), 80) || 'a dialog' : '',
    lines: out,
  };
};
const find = () => {
  if (A.ref != null && A.ref !== '') {
    const w = S.els.get(String(A.ref).replace(/[\[\]]/g, ''));
    const el = w && w.deref();
    if (el && el.isConnected) return el;
    throw new Error(`[${A.ref}] isn't on the page any more. Call browser_look for fresh numbers.`);
  }
  if (A.selector) {
    const el = document.querySelector(A.selector);
    if (!el) throw new Error(`Nothing matches ${A.selector}.`);
    return el;
  }
  if (A.text) {
    const want = String(A.text).toLowerCase().trim();
    const all = [...document.querySelectorAll(CONTROLS)].filter((e) => control(e) && shown(e));
    // Its name, and what else a person would call it by: placeholder, tooltip, value.
    const said = (e) =>
      [nameOf(e), e.getAttribute('placeholder'), e.getAttribute('title'),
        e.tagName === 'SELECT' && e.selectedOptions[0] ? e.selectedOptions[0].text : '']
        .filter(Boolean).map((t) => clip(t, 500).toLowerCase()).join(' \u0001 ');
    const exact = (e) => said(e).split(' \u0001 ').includes(want);
    const el =
      all.find(exact) ||
      all.find((e) => said(e).includes(want)) ||
      [...document.querySelectorAll('body *')].find(
        (e) => !SKIP.has(e.tagName) && e.children.length === 0 && shown(e) &&
          clip(e.innerText, 500).toLowerCase().includes(want)
      );
    if (!el) throw new Error(`Nothing on the page says "${A.text}".`);
    return el;
  }
  throw new Error('Say which element: ref (its [number]), text or selector.');
};
const aim = () => {
  const el = find();
  el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
  const r = el.getBoundingClientRect();
  const x = r.left + r.width / 2, y = r.top + r.height / 2;
  const top = document.elementFromPoint(x, y);
  const covered = top && top !== el && !el.contains(top) && !top.contains(el);
  return {
    x, y,
    what: describe(el),
    tag: el.tagName,
    covered: covered ? clip(describe(top).replace(/^\[\d+\] /, ''), 80) : '',
  };
};
// Put the caret in the element and select what's in it, so typing replaces it.
const ready = () => {
  const el = find();
  el.focus();
  if (A.clear !== false) {
    if (typeof el.select === 'function' && 'value' in el) el.select();
    else if (el.isContentEditable) {
      const r = document.createRange();
      r.selectNodeContents(el);
      getSelection().removeAllRanges();
      getSelection().addRange(r);
    }
  }
  return { tag: el.tagName, what: describe(el), focused: document.activeElement === el };
};
const choose = () => {
  const el = find();
  if (el.tagName !== 'SELECT') throw new Error('That is not a select.');
  const want = String(A.value).toLowerCase().trim();
  const o = [...el.options].find((o) => o.text.toLowerCase().trim() === want || o.value.toLowerCase() === want) ||
    [...el.options].find((o) => o.text.toLowerCase().includes(want));
  if (!o) throw new Error(`No option "${A.value}". It has: ${[...el.options].map((o) => o.text).join(' | ')}`);
  el.value = o.value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return { what: describe(el) };
};
return { outline, aim, ready, choose }[OP]();
