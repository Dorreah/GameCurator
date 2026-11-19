// Application state and navigation (no view imports here to avoid cycles).

export const state = {
  info: null,      // app_info
  config: null,    // AppConfig
  facets: null,    // library counts, genres, tags
  view: null,
  params: {},
  archiveOk: true,
  query: { search: '', sort: 'title' },
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

export const CARD_WIDTH = { small: 132, medium: 168, large: 216 };

export const THEMES = {
  classic: { name: 'Classic', description: 'Dark big-box era launcher with a hint of violet' },
  amber: { name: 'Terminal CRT Amber', description: 'Amber phosphor terminal look' },
  matrix: { name: 'Matrix Terminal', description: 'Green digital-rain terminal look' },
  dos: { name: 'DOS Midnight', description: 'MS-DOS text mode, a shade darker – for the evening' },
};

/** Switches the visual theme (appearance only - nothing else changes). */
export function applyTheme(theme) {
  const t = THEMES[theme] ? theme : 'classic';
  const root = document.documentElement;
  root.dataset.theme = t;
  root.classList.toggle('term', t !== 'classic');
  try { localStorage.setItem('gc-theme', t); } catch (_) { /* cache only */ }
}
