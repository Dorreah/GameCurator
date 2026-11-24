// Archive counts: total and offline games for the status bar, plus the
// genres (each of a comma list on its own) and tags for the library filter
// and the tag suggestions of the game form.

import { api } from './api.js';
import { state } from './store.js';
import { t, tn } from './i18n.js';

export async function refreshFacets() {
  try {
    state.facets = await api.facets();
  } catch (_) {
    return; // archive unavailable - the status bar reports it
  }
  const c = document.getElementById('sb-count');
  if (c) {
    const f = state.facets;
    c.textContent = `${tn(f.total, '{n} game', '{n} games')}${f.unavailable ? ` · ${t('{n} offline', { n: f.unavailable })}` : ''}`;
    c.title = f.unavailable ? t('Offline: the folder of the game is not reachable right now (drive disconnected?).') : '';
  }
}
