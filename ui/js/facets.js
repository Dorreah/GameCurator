// Archive counts: total and offline games for the status bar, plus the
// existing genres and tags offered as suggestions in the game form.
// (There are deliberately no filters - the library is one whole archive.)

import { api } from './api.js';
import { state } from './store.js';

export async function refreshFacets() {
  try {
    state.facets = await api.facets();
  } catch (_) {
    return; // archive unavailable - the status bar reports it
  }
  const c = document.getElementById('sb-count');
  if (c) {
    const f = state.facets;
    c.textContent = `${f.total} game${f.total === 1 ? '' : 's'}${f.unavailable ? ` · ${f.unavailable} offline` : ''}`;
    c.title = f.unavailable ? 'Offline: the folder of the game is not reachable right now (drive disconnected?).' : '';
  }
}
