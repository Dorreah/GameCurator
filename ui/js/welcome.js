// The welcome window at start: what Game Curator is about, and the spirit it
// is meant to be used in. "Don't show this again" is stored in config.json
// (Settings can turn it back on).

import { api } from './api.js';
import { openModal } from './ui.js';
import { state } from './store.js';

export async function openWelcome() {
  const m = openModal({
    title: 'Welcome',
    width: 640,
    body: `<div class="welcome">
      <div class="welcome-mark" aria-hidden="true"></div>
      <h1>Welcome, curator</h1>
      <p>Game Curator is a quiet archive for the games you own – their installers, manuals and extras, and everything
      you find out about them, kept in order on your own drive.</p>
      <p>Approach your collection like a <b>video game archivist and researcher</b>: open a game, look closely, find out
      how it was made and why it works the way it does – and write it down.</p>
      <p>And take your time. Games are made to be <b>experienced</b> – slowly, attentively, with all their atmosphere.
      Not to rack up hours, achievements or numbers. Nothing here is waiting to be finished.</p>
      <p>Curating is also a quiet kind of training: in <b>deep focus</b>, in <b>analysis</b>, and in <b>noticing the small
      details</b> that make a game what it is.</p>
      <p class="welcome-motto">Your games. Your archive. Your history.</p>
    </div>`,
    footer: `<label class="check left"><input type="checkbox" id="wl-hide"> Don't show this again
        <span class="hint">(Settings can bring it back)</span></label>
      <button class="btn gold" id="wl-enter">Enter the archive</button>`,
  });
  m.el.querySelector('.window').classList.add('welcome-window');
  const enter = m.el.querySelector('#wl-enter');
  enter.addEventListener('click', () => m.close(true));
  enter.focus();
  await m.result;
  if (m.el.querySelector('#wl-hide').checked && state.config) {
    try {
      state.config = await api.saveConfig({ ...state.config, showWelcome: false });
    } catch (_) { /* the window simply shows again next time */ }
  }
}
