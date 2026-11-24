// The welcome window at start: what Game Curator is about, and the spirit it
// is meant to be used in. "Don't show this again" is stored in config.json
// (Settings can turn it back on). The language can be switched right here.

import { api } from './api.js';
import { openModal } from './ui.js';
import { state, changeLanguage } from './store.js';
import { t, langSwitch } from './i18n.js';

function body() {
  return `<div class="welcome">
      <div class="wz-lang">${langSwitch('wl-lang')}</div>
      <div class="welcome-mark" aria-hidden="true"></div>
      <h1>${t('Welcome, curator')}</h1>
      <p>${t('Game Curator is a quiet archive for the games you own – their installers, manuals and extras, and everything you find out about them, kept in order on your own drive.')}</p>
      <p>${t('Approach your collection like a <b>video game archivist and researcher</b>: open a game, look closely, find out how it was made and why it works the way it does – and write it down.')}</p>
      <p>${t('And take your time. Games are made to be <b>experienced</b> – slowly, attentively, with all their atmosphere. Not to rack up hours, achievements or numbers. Nothing here is waiting to be finished.')}</p>
      <p>${t('Curating is also a quiet kind of training: in <b>deep focus</b>, in <b>analysis</b>, and in <b>noticing the small details</b> that make a game what it is.')}</p>
      <p class="welcome-motto">${t('Your games. Your archive. Your history.')}</p>
    </div>`;
}

function footer() {
  return `<label class="check left"><input type="checkbox" id="wl-hide"> <span id="wl-hide-label">${t("Don't show this again")}</span>
        <span class="hint" id="wl-hide-hint">${t('(Settings can bring it back)')}</span></label>
      <button class="btn gold" id="wl-enter">${t('Enter the archive')}</button>`;
}

export async function openWelcome() {
  const m = openModal({ title: t('Welcome'), width: 640, body: body(), footer: footer() });
  m.el.querySelector('.window').classList.add('welcome-window');
  const enter = m.el.querySelector('#wl-enter');
  enter.addEventListener('click', () => m.close(true));
  enter.focus();

  function wireLang() {
    m.body.querySelectorAll('input[name=wl-lang]').forEach((r) => r.addEventListener('change', async () => {
      await changeLanguage(r.value);
      m.el.querySelector('.window-title span').textContent = t('Welcome');
      m.body.innerHTML = body();
      m.el.querySelector('#wl-hide-label').textContent = t("Don't show this again");
      m.el.querySelector('#wl-hide-hint').textContent = t('(Settings can bring it back)');
      enter.textContent = t('Enter the archive');
      wireLang();
      m.body.querySelector(`input[name=wl-lang][value="${r.value}"]`)?.focus();
    }));
  }
  wireLang();

  await m.result;
  if (m.el.querySelector('#wl-hide').checked && state.config) {
    try {
      state.config = await api.saveConfig({ ...state.config, showWelcome: false });
    } catch (_) { /* the window simply shows again next time */ }
  }
}
