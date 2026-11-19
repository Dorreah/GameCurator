// First-run setup wizard (can be skipped at any step).

import { api } from './api.js';
import { esc, openModal, toast } from './ui.js';
import { state, navigate } from './store.js';

const STEPS = ['Welcome', 'Your games', 'Done'];

export function openWizard(rerun = false) {
  let step = 0;
  let info = state.info;
  let gamesFolder = (state.config && state.config.scanRoot) || info.gamesDir;
  let scanNow = false;

  const m = openModal({
    title: 'Game Curator – Setup',
    width: 720,
    closable: rerun,
    body: '<div class="wizard"><ol class="wizard-steps" id="wz-steps"></ol><div class="wizard-page" id="wz-page"></div></div>',
    footer: `<div class="left"><button class="btn" id="wz-skip">Skip</button></div>
      <button class="btn" id="wz-back">◄ Back</button><button class="btn gold" id="wz-next">Next ►</button>`,
  });
  m.body.style.padding = '0';
  const q = (id) => m.el.querySelector(`#${id}`);

  function page() {
    switch (step) {
      case 0:
        return `<h2>Welcome to Game Curator</h2>
          <p>A quiet, offline place to keep the installers, manuals and extras of your games in order – and what you know about them.</p>
          <dl class="facts" style="grid-template-columns:110px 1fr;margin-top:14px">
            <dt>Runs from</dt><dd><span class="mono">${esc(info.archiveRoot)}</span><div class="hint">${esc(info.driveKind)}</div></dd>
            <dt>Storage</dt><dd>${info.writable
              ? '<span class="led on"></span> writable'
              : '<span class="led off"></span> <b>not writable</b> – remove the write protection of the drive'}</dd>
            <dt>Catalog</dt><dd>${info.dbCreated ? 'new, empty catalog created' : `${info.gameCount} game(s) in the catalog`}</dd>
          </dl>
          <p class="hint" style="margin-top:14px">Nothing is installed: no registry entries, nothing in AppData. Move the whole folder and everything keeps working.</p>`;
      case 1:
        return `<h2>Where are your games?</h2>
          <p>The folder with your installers – for example <span class="mono">Games</span> next to the program, or <span class="mono">GOG_Installers</span> on the same drive.</p>
          <div class="row"><input class="field mono" id="wz-folder" value="${esc(gamesFolder)}"><button class="btn" id="wz-browse">Browse…</button></div>
          <label class="check" style="margin-top:14px"><input type="checkbox" id="wz-scan" ${scanNow ? 'checked' : ''}> Look for games in this folder when setup is finished</label>
          <p class="hint">You review what was found before anything is added. Nothing is ever run, moved or changed.</p>`;
      default:
        return `<h2>Ready</h2>
          <p>Add games one by one with <b>Add Game</b>, or let the <b>Archive Scanner</b> find them.
          Each game keeps its files, artwork, tags, a journal and what you know about it.</p>
          ${scanNow ? '<p>The scan starts now.</p>' : ''}
          <p class="muted" style="font-style:italic">Your games. Your archive. Your history.</p>`;
    }
  }

  function render() {
    q('wz-steps').innerHTML = STEPS.map((s, i) => `<li class="${i < step ? 'done' : i === step ? 'current' : ''}">${i + 1}. ${s}</li>`).join('');
    q('wz-page').innerHTML = page();
    q('wz-back').disabled = step === 0;
    q('wz-next').textContent = step === STEPS.length - 1 ? 'Finish' : 'Next ►';
    q('wz-skip').hidden = step === STEPS.length - 1;
    if (step === 1) {
      q('wz-browse').addEventListener('click', async () => {
        try {
          const p = await api.pickFolder('Choose your games folder', q('wz-folder').value);
          if (p) q('wz-folder').value = p;
        } catch (e) { toast(String(e), 'err'); }
      });
    }
  }

  function collect() {
    if (step === 1) {
      gamesFolder = q('wz-folder').value.trim() || info.gamesDir;
      scanNow = q('wz-scan').checked;
    }
  }

  async function finish(skipped) {
    try {
      const patch = { ...state.config, wizardCompleted: true };
      if (!skipped) patch.scanRoot = gamesFolder;
      state.config = await api.saveConfig(patch);
    } catch (e) {
      toast(`Settings could not be saved: ${e}`, 'err');
    }
    m.close(true);
    if (!skipped && scanNow) navigate('scanner', { autoStart: true });
  }

  q('wz-next').addEventListener('click', () => {
    collect();
    if (step === STEPS.length - 1) { finish(false); return; }
    step += 1;
    render();
  });
  q('wz-back').addEventListener('click', () => { collect(); step = Math.max(0, step - 1); render(); });
  q('wz-skip').addEventListener('click', () => finish(true));

  api.appInfo().then((fresh) => { info = fresh; state.info = fresh; render(); }).catch(() => render());
  render();
  return m.result;
}
