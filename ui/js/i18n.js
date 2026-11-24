// Interface language. The English text in the source is the key; the Czech
// translations live in lang/cs.js. Anything without a translation stays in
// English, so a missing entry never breaks the interface.
//
// Every window (main, PDF viewer, music player, journal offer) reads the
// language from localStorage synchronously ('gc-lang', a cache written by the
// main window from config.json) and follows changes through storage events.

import { CS, CS_MESSAGES } from './lang/cs.js';

export const LANGUAGES = { en: 'English', cs: 'Čeština' };

const DICTS = { cs: CS };
const MESSAGES = { cs: CS_MESSAGES };

let lang = 'en';
try { lang = localStorage.getItem('gc-lang') === 'cs' ? 'cs' : 'en'; } catch (_) { /* storage unavailable */ }
document.documentElement.lang = lang;

export function getLang() {
  return lang;
}

/** Switches the language of this window (the caller re-renders its views). */
export function setLang(next) {
  lang = next === 'cs' ? 'cs' : 'en';
  try { localStorage.setItem('gc-lang', lang); } catch (_) { /* cache only */ }
  document.documentElement.lang = lang;
  applyStatic();
}

function fill(s, params) {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (m, k) => (Object.prototype.hasOwnProperty.call(params, k) ? String(params[k]) : m));
}

/**
 * Marks an English text for translation without translating it yet (for
 * tables defined once and shown later through t() or tn()). It returns the
 * text unchanged; tools/check_i18n.py collects these keys.
 */
export const mark = (text) => text;

/** Translates `text` (English) and fills {placeholders} from `params`. */
export function t(text, params) {
  const d = DICTS[lang];
  let v = d ? d[text] : undefined;
  if (Array.isArray(v)) v = v[v.length - 1];
  return fill(typeof v === 'string' ? v : text, params);
}

/**
 * Plural forms. English has one/other; Czech has one (1), few (2-4) and
 * many (0, 5+). The dictionary key is the English `other` form and its
 * Czech value is [one, few, many]. `{n}` is always available.
 */
export function tn(n, one, other, params = {}) {
  const p = { n, ...params };
  const d = DICTS[lang];
  const v = d ? d[other] : undefined;
  if (Array.isArray(v)) {
    const i = n === 1 ? 0 : (n >= 2 && n <= 4 ? 1 : 2);
    return fill(v[i], p);
  }
  if (typeof v === 'string') return fill(v, p);
  return fill(n === 1 ? one : other, p);
}

// ---------------------------------------------------------------- messages from Rust

let patterns = null;
function compiled() {
  if (patterns) return patterns;
  const d = MESSAGES[lang] || {};
  patterns = Object.keys(d)
    .filter((k) => k.includes('{0}'))
    .sort((a, b) => b.length - a.length)
    .map((k) => {
      const parts = k.split(/\{\d\}/).map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
      return { re: new RegExp(`^${parts.join('([\\s\\S]*?)')}$`), value: d[k] };
    });
  return patterns;
}
let patternLang = lang;

/**
 * Translates a message produced by the Rust side (errors, notes). The
 * messages are English sentences; known ones are matched exactly or by
 * pattern ({0} = variable part, translated again when it is a message too).
 */
export function tm(message, depth = 0) {
  const msg = String(message ?? '');
  if (lang === 'en' || !msg) return msg;
  if (patternLang !== lang) { patterns = null; patternLang = lang; }
  const d = MESSAGES[lang] || {};
  if (Object.prototype.hasOwnProperty.call(d, msg)) return d[msg];
  for (const { re, value } of compiled()) {
    const m = re.exec(msg);
    if (m) {
      return value.replace(/\{(\d)\}/g, (_, i) => {
        const part = m[Number(i) + 1] ?? '';
        return depth < 2 ? tm(part, depth + 1) : part;
      });
    }
  }
  return msg;
}

// ---------------------------------------------------------------- static HTML

/**
 * Translates elements marked in HTML: data-i18n (text), data-i18n-title,
 * data-i18n-placeholder, data-i18n-aria (aria-label). The English original
 * is kept in the attribute, so switching back works.
 */
export function applyStatic(root = document) {
  root.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
  root.querySelectorAll('[data-i18n-title]').forEach((el) => { el.title = t(el.dataset.i18nTitle); });
  root.querySelectorAll('[data-i18n-placeholder]').forEach((el) => { el.placeholder = t(el.dataset.i18nPlaceholder); });
  root.querySelectorAll('[data-i18n-aria]').forEach((el) => { el.setAttribute('aria-label', t(el.dataset.i18nAria)); });
  const title = document.querySelector('title[data-i18n-doc]');
  if (title) document.title = t(title.dataset.i18nDoc);
}

// Module scripts run once the document is parsed: translate the static
// parts right away, before the first paint.
if (lang !== 'en') applyStatic();

/** Small English | Čeština switch (radio buttons named `name`). */
export function langSwitch(name) {
  return `<span class="lang-switch" role="radiogroup" aria-label="Language">${Object.entries(LANGUAGES).map(([key, label]) => `
    <label class="check"><input type="radio" name="${name}" value="${key}" ${lang === key ? 'checked' : ''}> ${label}</label>`).join('')}</span>`;
}

/** Calls `fn` when another window switches the language. */
export function onLangChange(fn) {
  window.addEventListener('storage', (e) => {
    if (e.key !== 'gc-lang') return;
    const next = e.newValue === 'cs' ? 'cs' : 'en';
    if (next === lang) return;
    setLang(next);
    fn(next);
  });
}
