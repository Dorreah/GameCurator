// Shared UI helpers: escaping, formatting, modal windows, notifications.

import { t, tm, mark } from './i18n.js';

/** File categories; the labels are English keys - use categoryLabel(). */
export const CATEGORY_LABELS = {
  installer: mark('Installer'),
  executable: mark('Program'),
  data: mark('Data'),
  manual: mark('Manual'),
  image: mark('Image'),
  music: mark('Music'),
  archive: mark('Archive'),
  text: mark('Text'),
  other: mark('Other'),
};

export function categoryLabel(c) {
  return t(CATEGORY_LABELS[c] || CATEGORY_LABELS.other);
}

/** Order, heading, singular and plural of the file groups in a game's archive
 * (English keys; the plural is also the key of the Czech plural forms). */
export const CATEGORY_GROUPS = [
  ['installer', mark('Installers'), mark('{n} installer'), mark('{n} installers')],
  ['executable', mark('Programs'), mark('{n} program'), mark('{n} programs')],
  ['data', mark('Installer data'), mark('{n} data file'), mark('{n} data files')],
  ['manual', mark('Manuals'), mark('{n} manual'), mark('{n} manuals')],
  ['music', mark('Music'), mark('{n} music file'), mark('{n} music files')],
  ['image', mark('Images'), mark('{n} image'), mark('{n} images')],
  ['archive', mark('Archives'), mark('{n} archive'), mark('{n} archives')],
  ['text', mark('Texts'), mark('{n} text'), mark('{n} texts')],
  ['other', mark('Other files'), mark('{n} other file'), mark('{n} other files')],
];

const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ESC[c]);
}

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

export function formatBytes(n) {
  if (n == null) return '';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = Number(n);
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${u[i]}`;
}

export function todayIso() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function formatDate(iso) {
  if (!iso) return '';
  const [y, m, d] = iso.slice(0, 10).split('-');
  return `${Number(d)}. ${Number(m)}. ${y}`;
}

export function initials(title) {
  const words = String(title).replace(/[^\p{L}\p{N} ]/gu, ' ').split(/\s+/).filter(Boolean);
  return (words.slice(0, 2).map((w) => w[0]).join('') || '?').toUpperCase();
}

export function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}

// ---------------------------------------------------------------- status bar

let msgTimer;
export function status(msg, kind = '') {
  const el = document.getElementById('sb-msg');
  if (!el) return;
  el.textContent = msg;
  el.className = `sb-msg ${kind}`;
  el.title = msg;
  clearTimeout(msgTimer);
  if (kind) msgTimer = setTimeout(() => { el.textContent = t('Ready.'); el.className = 'sb-msg'; }, 8000);
}

let toastTimer;
export function toast(msg, kind = 'ok') {
  let el = document.getElementById('toast');
  if (!el) {
    el = document.createElement('div');
    el.id = 'toast';
    document.body.appendChild(el);
  }
  el.className = `toast ${kind}`;
  el.textContent = msg;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, kind === 'err' ? 7000 : 3500);
  status(msg, kind === 'err' ? 'err' : 'ok');
}

// ---------------------------------------------------------------- modal windows

const stack = [];

/**
 * Opens a modal window. `body` is an HTML string or element. Returns an
 * object with `el`, `close(value)` and `result` (a Promise).
 */
export function openModal({ title, body, footer = '', width = 520, closable = true, onKey }) {
  const root = document.getElementById('modal-root');
  const overlay = document.createElement('div');
  overlay.className = 'overlay';
  overlay.innerHTML = `
    <div class="window" role="dialog" aria-modal="true" style="width:${width}px">
      <div class="window-title"><span>${esc(title)}</span><span class="spacer"></span>
        ${closable ? `<button class="btn x" data-close title="${esc(t('Close'))}">✕</button>` : ''}</div>
      <div class="window-body"></div>
      ${footer ? `<div class="window-foot">${footer}</div>` : ''}
    </div>`;
  const bodyEl = overlay.querySelector('.window-body');
  if (typeof body === 'string') bodyEl.innerHTML = body; else bodyEl.appendChild(body);
  root.appendChild(overlay);

  let resolve;
  const result = new Promise((r) => { resolve = r; });
  const modal = {
    el: overlay,
    body: bodyEl,
    result,
    close(value) {
      overlay.remove();
      const i = stack.indexOf(modal);
      if (i >= 0) stack.splice(i, 1);
      document.removeEventListener('keydown', keyHandler, true);
      resolve(value);
    },
  };
  function keyHandler(e) {
    if (stack[stack.length - 1] !== modal) return;
    if (onKey && onKey(e, modal) === true) return;
    if (e.key === 'Escape' && closable) { e.preventDefault(); e.stopPropagation(); modal.close(undefined); }
  }
  document.addEventListener('keydown', keyHandler, true);
  overlay.querySelectorAll('[data-close]').forEach((b) => b.addEventListener('click', () => modal.close(undefined)));
  stack.push(modal);
  setTimeout(() => {
    const f = overlay.querySelector('[autofocus], .window-body input, .window-body select, .window-body textarea, .window-foot .btn.gold');
    if (f) f.focus();
  }, 0);
  return modal;
}

export function modalOpen() {
  return stack.length > 0;
}

/** Message box. kind: info | warn | err */
export function alertBox(message, { title = 'Game Curator', kind = 'info' } = {}) {
  const icon = { info: 'i', warn: '!', err: '✕' }[kind] || 'i';
  const m = openModal({
    title,
    width: 460,
    body: `<div class="dlg-icon ${kind}">${icon}</div><div class="dlg-text">${esc(message)}</div>`,
    footer: '<button class="btn gold" data-ok autofocus>OK</button>',
  });
  m.el.querySelector('[data-ok]').addEventListener('click', () => m.close(true));
  return m.result;
}

/**
 * Confirmation dialog. Resolves to true/false. `html` allows rich body
 * content (already escaped by the caller); `danger` styles the OK button.
 */
export function confirmBox({ title = t('Please confirm'), message = '', html = '', ok = 'OK', cancel = t('Cancel'), kind = 'warn', danger = false, width = 480 }) {
  const icon = { info: 'i', warn: '!', err: '✕' }[kind] || '!';
  const m = openModal({
    title,
    width,
    body: `<div class="dlg-icon ${kind}">${icon}</div><div class="dlg-text">${html || esc(message)}</div>`,
    footer: `<button class="btn" data-cancel>${esc(cancel)}</button><button class="btn ${danger ? 'danger' : 'gold'}" data-ok>${esc(ok)}</button>`,
  });
  m.el.querySelector('[data-ok]').addEventListener('click', () => m.close(true));
  m.el.querySelector('[data-cancel]').addEventListener('click', () => m.close(false));
  setTimeout(() => m.el.querySelector('[data-cancel]').focus(), 0);
  return m.result.then((v) => v === true);
}

/** Runs an async action, reporting errors in a message box. */
export async function guard(fn, { errorTitle = t('Something went wrong') } = {}) {
  try {
    return await fn();
  } catch (e) {
    const msg = tm(typeof e === 'string' ? e : (e && e.message) || String(e));
    status(msg, 'err');
    await alertBox(msg, { title: errorTitle, kind: 'err' });
    return undefined;
  }
}

/** Small checkbox icon: is the game installed on this computer? */
export function installBox(on, extraClass = '') {
  const title = on ? t('Installed on this PC') : t('Not installed on this PC');
  return `<span class="inst-box ${on ? 'on' : 'off'} ${extraClass}" title="${title}" aria-label="${title}" role="img">`
    + '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="1.5" width="13" height="13" rx="2"/>'
    + '<path d="M4.4 8.4l2.5 2.5 4.9-5.4"/></svg></span>';
}
