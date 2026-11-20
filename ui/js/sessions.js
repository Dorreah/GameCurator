// After playing: when a game started with ▶ Play has ended, Game Curator
// offers to write a journal entry while the impressions are fresh - in a
// small window on top of all others (offer.html). Only games started from
// here are noticed, and no play time is counted or shown.

import { api, listen } from './api.js';
import { esc, guard, openModal, toast } from './ui.js';
import { state, navigate } from './store.js';

function runningChanged() {
  document.dispatchEvent(new CustomEvent('gc-running-changed'));
}

/** The offer as a dialog in the main window (fallback). Resolves when it closes. */
async function offerJournal({ gameId, title }) {
  const m = openModal({
    title: 'Back from the game',
    width: 500,
    body: `<div class="dlg-icon info">✎</div><div class="dlg-text" id="jo-text">You just finished playing <b>${esc(title)}</b>.

Would you like to write down your impressions in the Gaming Journal while they are still fresh – a discovery, a moment, a thought about the game?</div>
      <label class="check" style="margin-top:14px;clear:both"><input type="checkbox" id="jo-never"> Don't show this again</label>`,
    footer: '<button class="btn" data-close id="jo-later">Not now</button><button class="btn gold" id="jo-write">✎ Write entry</button>',
  });
  const q = (sel) => m.el.querySelector(sel);
  q('#jo-write').addEventListener('click', () => m.close('write'));
  setTimeout(() => q('#jo-write').focus(), 0);
  const choice = await m.result;
  if (q('#jo-never').checked) {
    await guard(async () => {
      state.config = await api.saveConfig({ ...state.config, journalPrompt: false });
      toast('The journal offer after playing is turned off. You can turn it back on in Settings > Gaming Journal.');
    });
  }
  if (choice === 'write') {
    await navigate('profile', { id: gameId });
    document.getElementById('pf-new-entry')?.click();
  }
}

let queue = Promise.resolve();

/** "Write entry" in the offer window: a new journal entry for the game. */
async function writeEntry({ gameId }) {
  await navigate('profile', { id: gameId });
  document.getElementById('pf-new-entry')?.click();
}

export async function initSessions() {
  try { (await api.runningGames()).forEach((id) => state.running.add(id)); } catch (_) { /* nothing running */ }
  listen('gc-game-started', (p) => { state.running.add(p.gameId); runningChanged(); });
  listen('gc-game-stopped', (p) => { state.running.delete(p.gameId); runningChanged(); });
  // the offer itself is a window of its own, on top of all others (Rust: offer.rs)
  listen('gc-journal-write', (p) => { queue = queue.then(() => writeEntry(p)).catch(() => {}); });
  listen('gc-config-changed', async () => {
    try { state.config = await api.getConfig(); } catch (_) { return; }
    const box = document.getElementById('st-journal-prompt');
    if (box) box.checked = state.config.journalPrompt !== false;
  });
  // only if that window could not be opened: the dialog in this window
  listen('gc-offer-fallback', (p) => {
    if (state.config?.journalPrompt === false) return;
    queue = queue.then(() => offerJournal(p)).catch(() => {});
  });
}
