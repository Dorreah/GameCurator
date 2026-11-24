// Application state and navigation (no view imports here to avoid cycles).

import { t, mark, getLang, setLang } from './i18n.js';
import { api } from './api.js';

export const state = {
  info: null,      // app_info
  config: null,    // AppConfig
  facets: null,    // library counts, genres, tags
  view: null,
  params: {},
  archiveOk: true,
  query: { search: '', sort: 'title', genres: [], tags: [] },
  running: new Set(), // games started with Play that are still running
};

export const views = {};
const navListeners = [];
let leaveGuard = null;

export function onNavigate(fn) {
  navListeners.push(fn);
}

/** A view can register a function that runs (and may save) before leaving. */
export function setLeaveGuard(fn) {
  leaveGuard = fn;
}

export async function navigate(name, params = {}) {
  if (leaveGuard) {
    const g = leaveGuard;
    leaveGuard = null;
    try { await g(); } catch (_) { /* reported by the guard itself */ }
  }
  const view = views[name];
  if (!view) return;
  state.view = name;
  state.params = params;
  navListeners.forEach((fn) => fn(name, params));
  await view.show(params);
}

/**
 * Switches the interface language: saves it in config.json, relabels the
 * window chrome (app.js listens to 'gc-lang') and shows the current view
 * again in the new language.
 */
export async function changeLanguage(next) {
  setLang(next);
  document.dispatchEvent(new CustomEvent('gc-lang'));
  if (state.config && state.config.language !== getLang()) {
    try {
      state.config = await api.saveConfig({ ...state.config, language: getLang() });
    } catch (_) { /* kept for this session */ }
  }
  const view = views[state.view];
  if (view) await view.show(state.params);
}

export const CARD_WIDTH = { small: 132, medium: 168, large: 216 };

export const THEMES = {
  classic: { name: 'Classic', description: mark('VGA-era DOS launcher: pixel font, bevels, a hint of violet') },
  amber: { name: 'Terminal CRT Amber', description: mark('Soft amber terminal with warm white and light red') },
  matrix: { name: 'Matrix Terminal', description: mark('Soft green digital rain with white and yellow') },
  dos: { name: 'DOS Midnight', description: mark('MS-DOS text mode with the IBM VGA font, a shade darker') },
};

/** Fonts of the Classic look (Settings > Appearance). Names and notes are
 * English keys, translated where they are shown. */
export const CLASSIC_FONTS = {
  'pixel-operator': { name: 'Pixel Operator', note: mark('pixel font, 16 px grid (default)') },
  dogica: { name: 'Dogica', note: mark('Game Boy-style 8 px pixel font') },
  'geist-pixel': { name: 'Geist Pixel', note: mark('modern pixel design, drawn smoothly') },
  'roboto-slab': { name: 'Roboto Slab', note: mark('calm slab serif for long reading') },
};

/** Switches the font of the Classic look (the text-mode themes keep theirs). */
export function applyClassicFont(font) {
  const f = CLASSIC_FONTS[font] ? font : 'pixel-operator';
  document.documentElement.dataset.classicFont = f;
  try { localStorage.setItem('gc-classic-font', f); } catch (_) { /* cache only */ }
}

/** Switches the visual theme (appearance only - nothing else changes). */
export function applyTheme(theme) {
  const t = THEMES[theme] ? theme : 'classic';
  const root = document.documentElement;
  root.dataset.theme = t;
  root.classList.toggle('term', t !== 'classic');
  try { localStorage.setItem('gc-theme', t); } catch (_) { /* cache only */ }
}

const AUDIO = ['mp3', 'ogg', 'oga', 'opus', 'flac', 'wav', 'm4a', 'aac'];

/** Tooltip of an Open button: PDFs and music open in Game Curator itself. */
export function openHint(name) {
  const ext = String(name).split('.').pop().toLowerCase();
  const own = state.config?.builtinViewer !== false;
  if (ext === 'pdf') return own ? t('Read in the Game Curator viewer') : t('Open with the default application');
  if (AUDIO.includes(ext)) return own ? t('Play in the Game Curator music player') : t('Open with the default application');
  return t('Open with the default application');
}
