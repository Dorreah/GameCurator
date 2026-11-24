// Settings: appearance, library display, archive, backups.

import { api } from './api.js';
import { esc, formatBytes, guard, toast, confirmBox } from './ui.js';
import { state, THEMES, CLASSIC_FONTS, applyTheme, applyClassicFont, changeLanguage } from './store.js';
import { refreshFacets } from './facets.js';
import { openWizard } from './wizard.js';
import { t, tn, tm, getLang, LANGUAGES } from './i18n.js';

async function save(patch, message) {
  await guard(async () => {
    state.config = await api.saveConfig({ ...state.config, ...patch });
    toast(typeof message === 'function' ? message() : message || t('Settings saved.'));
  });
}

const SWATCH_TEXT = {
  classic: () => `<b>Game<br>Curator</b><span>${t('Personal game archive')}</span>`,
  amber: () => '<b><i>C:\\&gt;</i> GAME CURATOR<u>█</u></b><span>&gt; READY.</span>',
  matrix: () => '<b><i>&gt;</i> GAME CURATOR<u>█</u></b><span>&gt; wake up, curator…</span>',
  dos: () => `<b><i>A:\\&gt;</i> GAME CURATOR_</b><span>Alt+1 ${t('Library')} · Alt+5</span>`,
};

function themePicker(current) {
  return `<div class="theme-picker" id="st-theme">${Object.entries(THEMES).map(([key, th]) => `
    <label class="theme-option" title="${esc(t(th.description))}">
      <input type="radio" name="theme" value="${key}" ${current === key ? 'checked' : ''}>
      <div class="theme-swatch ${key}">${SWATCH_TEXT[key]()}</div>
      <span class="theme-name">${esc(th.name)}</span><span class="hint">${esc(t(th.description))}</span>
    </label>`).join('')}</div>`;
}

const FONT_SAMPLE = 'Game Curator – Příliš žluťoučký kůň';

function fontPicker(current) {
  return `<div class="font-picker" id="st-font">${Object.entries(CLASSIC_FONTS).map(([key, f]) => `
    <label class="font-option">
      <input type="radio" name="classicFont" value="${key}" ${current === key ? 'checked' : ''}>
      <span class="font-sample font-${key}">${esc(FONT_SAMPLE)}</span>
      <span class="font-name">${esc(f.name)}</span><span class="hint">${esc(t(f.note))}</span>
    </label>`).join('')}</div>`;
}

function radio(name, value, current, label) {
  return `<label class="check" style="margin-right:16px"><input type="radio" name="${name}" value="${value}" ${current === value ? 'checked' : ''}> ${label}</label>`;
}

async function renderBackups() {
  const el = document.getElementById('st-backups');
  if (!el) return;
  let list = [];
  try { list = await api.listBackups(); } catch (e) { el.innerHTML = `<div class="notice err">${esc(e)}</div>`; return; }
  if (!list.length) { el.innerHTML = `<div class="muted" style="padding:8px">${t('No backups yet.')}</div>`; return; }
  const kind = { manual: t('Manual backup'), auto: t('Automatic'), 'pre-restore': t('Before restore'), 'pre-upgrade': t('Before upgrade') };
  el.innerHTML = `<table class="list"><thead><tr><th>${t('Created')}</th><th>${t('Type')}</th><th class="num">${t('Size')}</th><th></th></tr></thead><tbody>
    ${list.map((b) => `<tr><td>${esc(b.created)}</td><td>${esc(kind[b.kind] || b.kind)}</td><td class="num">${formatBytes(b.sizeBytes)}</td>
      <td class="act"><button class="btn small" data-restore="${esc(b.fileName)}" data-created="${esc(b.created)}">${t('Restore…')}</button></td></tr>`).join('')}
    </tbody></table>`;
}

export const settingsView = {
  async show(params = {}) {
    const cfg = state.config;
    const info = await api.appInfo().catch(() => state.info);
    state.info = info;
    const main = document.getElementById('main');
    const lang = getLang();
    main.innerHTML = `<div class="view"><div class="strip"><span>${t('Settings')}</span></div>
    <div class="library-scroll"><div class="settings">
      <fieldset class="group"><legend>${t('Language')}</legend>
        <div class="row lang-row" id="st-lang">${Object.entries(LANGUAGES).map(([key, name]) => `
          <label class="check" style="margin-right:18px"><input type="radio" name="language" value="${key}" ${lang === key ? 'checked' : ''}> ${esc(name)}</label>`).join('')}</div>
        <div class="hint" style="margin-top:4px">${t('Language of the whole interface. Game titles, descriptions and your own notes stay as you wrote them.')}</div>
      </fieldset>

      <fieldset class="group"><legend>${t('Appearance')}</legend>
        ${themePicker(cfg.theme)}
        <p class="hint" style="margin:10px 0 0">${t('Themes change only the look. Layout, controls and keyboard shortcuts stay the same.')}</p>
        <div class="form-label" style="margin-top:16px">${t('Font of the Classic look')}</div>
        ${fontPicker(cfg.classicFont || 'pixel-operator')}
        <p class="hint" id="st-font-note" style="margin:8px 0 0">${cfg.theme === 'classic' ? t('Used everywhere in the Classic look.') : t('Used when the Classic look is selected; the other themes keep their own font.')}</p>
        <label class="check" style="margin-top:12px"><input type="checkbox" id="st-welcome" ${cfg.showWelcome !== false ? 'checked' : ''}>
          ${t('Show the welcome window when Game Curator starts')}</label>
      </fieldset>

      <fieldset class="group"><legend>${t('Gaming Journal')}</legend>
        <label class="check"><input type="checkbox" id="st-journal-prompt" ${cfg.journalPrompt !== false ? 'checked' : ''}>
          ${t('After playing, offer to write a journal entry')}</label>
        <div class="hint" style="margin-top:4px">${t('When a game started with <b>▶ Play</b> ends, Game Curator asks whether you want to write down your impressions while they are fresh. It only notices games started from here, and never counts the time you play.')}</div>
      </fieldset>

      <fieldset class="group"><legend>${t('Manuals &amp; music')}</legend>
        <label class="check"><input type="checkbox" id="st-viewer" ${cfg.builtinViewer !== false ? 'checked' : ''}>
          ${t('Open PDF manuals and music in Game Curator')}</label>
        <div class="hint" style="margin-top:4px">${t('<b>Open</b> on a PDF shows it in a reader window, on a music file it plays the game\'s soundtrack in a small player &ndash; no other program needed. Both windows have <b>Open in default app</b>; switch this off to always use the programs set in Windows.')}</div>
      </fieldset>

      <fieldset class="group"><legend>${t('Library')}</legend>
        <div class="form-grid">
          <label>${t('Library view')}</label><div>${radio('viewMode', 'grid', cfg.viewMode, t('Grid View'))}${radio('viewMode', 'catalog', cfg.viewMode, t('Catalog View'))}</div>
          <label>${t('Card size')}</label><div>${radio('cardSize', 'small', cfg.cardSize, t('Small'))}${radio('cardSize', 'medium', cfg.cardSize, t('Medium'))}${radio('cardSize', 'large', cfg.cardSize, t('Large'))}</div>
        </div>
      </fieldset>

      <fieldset class="group"><legend>${t('Archive')}</legend>
        <div class="form-grid">
          <label>${t('Archive location')}</label><div><span class="led ${state.archiveOk ? 'on' : 'off'}"></span> <span class="mono">${esc(info.archiveRoot)}</span><div class="hint">${esc(tm(info.driveKind))} · ${t('all data is stored in <span class="mono">Data</span> next to GameCurator.exe')}</div></div>
          <label for="st-scanroot">${t('Scan folder')}</label>
          <div><div class="row"><input class="field mono" id="st-scanroot" value="${esc(cfg.scanRoot || '')}" placeholder="${esc(info.gamesDir)}"><button class="btn" id="st-scanroot-browse">${t('Browse…')}</button><button class="btn" id="st-scanroot-save">${t('Save')}</button></div>
            <div class="hint">${t('The folder the Archive Scanner opens with – usually the top folder of your game collection (e.g. <span class="mono">Games</span> on the archive drive).')}</div></div>
          <label>${t('Availability')}</label>
          <div><button class="btn" id="st-check">${t('Check archive availability')}</button><div id="st-check-result" style="margin-top:8px"></div></div>
          <label>${t('Installed games')}</label>
          <div><button class="btn" id="st-detect">${t('Look for installed games again')}</button>
            <div class="hint">${t('Game Curator looks at the GOG installation records and the Windows list of programs (read-only) each time it starts. Installations are remembered per computer (this one: <span class="mono">{machine}</span>).', { machine: esc(info.machine || '') })}</div>
            <div id="st-detect-result" style="margin-top:8px"></div></div>
          <label>${t('Setup')}</label><div><button class="btn" id="st-wizard">${t('Run the setup wizard again')}</button></div>
        </div>
      </fieldset>

      <fieldset class="group" id="st-details"><legend>${t('Game details from the internet')}</legend>
        <div class="form-grid">
          <label for="st-moby-key">${t('MobyGames API key')}</label>
          <div><div class="row"><input class="field mono" id="st-moby-key" type="password" autocomplete="off" spellcheck="false" maxlength="200"
                value="${esc(cfg.mobygamesKey || '')}" placeholder="${esc(t('not set'))}">
              <button class="btn" id="st-moby-show" type="button">${t('Show')}</button><button class="btn" id="st-moby-save" type="button">${t('Save')}</button></div>
            <div class="hint">${t('Needed only for English descriptions from <b>MobyGames</b> (Archive Scanner). MobyGames gives API keys for non-commercial use: sign in at mobygames.com, open your profile and click <b>API</b> &ndash; when asked which program the key is for, write <i>Game Curator</i>. The key is stored in <span class="mono">Data/config.json</span> next to GameCurator.exe and sent only to api.mobygames.com.')}</div></div>
        </div>
      </fieldset>

      <fieldset class="group"><legend>${t('Database &amp; backups')}</legend>
        <div class="row" style="margin-bottom:10px;flex-wrap:wrap">
          <button class="btn gold" id="st-backup">${t('Back up database now')}</button>
          <label class="check" style="margin-left:14px"><input type="checkbox" id="st-auto" ${cfg.autoBackup ? 'checked' : ''}> ${t('Automatic backup at start (at most once a day)')}</label>
          <label class="muted" for="st-keep" style="margin-left:10px">${t('keep')}</label>
          <select class="field" id="st-keep" style="flex:none;width:80px">${[3, 5, 10, 20, 30].map((n) => `<option ${n === cfg.autoBackupKeep ? 'selected' : ''}>${n}</option>`).join('')}</select>
        </div>
        <div class="backup-list" id="st-backups"></div>
        <div class="hint" style="margin-top:6px">${t('Backups are stored in <span class="mono">Data/Backups</span>. Restoring creates a safety backup of the current state first. Game folders are never affected.')}</div>
      </fieldset>

    </div></div></div>`;

    main.querySelectorAll('input[name=language]').forEach((r) => r.addEventListener('change', async () => {
      await changeLanguage(r.value);
      toast(t('Language: {name}', { name: LANGUAGES[r.value] }));
      document.querySelector(`#st-lang input[value="${r.value}"]`)?.focus();
    }));

    main.querySelectorAll('input[name=theme]').forEach((r) => r.addEventListener('change', () => {
      applyTheme(r.value);
      save({ theme: r.value }, t('Theme: {name}', { name: THEMES[r.value].name }));
    }));
    main.querySelectorAll('input[name=classicFont]').forEach((r) => r.addEventListener('change', () => {
      applyClassicFont(r.value);
      save({ classicFont: r.value }, t('Classic font: {name}', { name: CLASSIC_FONTS[r.value].name }));
    }));
    document.getElementById('st-journal-prompt').addEventListener('change', (e) => save({ journalPrompt: e.target.checked },
      e.target.checked ? t('Game Curator will offer a journal entry after playing.') : t('No journal offer after playing.')));
    document.getElementById('st-viewer').addEventListener('change', (e) => save({ builtinViewer: e.target.checked },
      e.target.checked ? t('PDFs and music open in Game Curator.') : t('PDFs and music open in the default applications.')));
    main.querySelectorAll('input[name=viewMode]').forEach((r) => r.addEventListener('change', () => save({ viewMode: r.value })));
    main.querySelectorAll('input[name=cardSize]').forEach((r) => r.addEventListener('change', () => save({ cardSize: r.value })));
    document.getElementById('st-scanroot-browse').addEventListener('click', () => guard(async () => {
      const p = await api.pickFolder(t('Choose the default folder for the Archive Scanner'), document.getElementById('st-scanroot').value || info.gamesDir);
      if (p) { document.getElementById('st-scanroot').value = p; await save({ scanRoot: p }, t('Scan folder saved.')); }
    }));
    document.getElementById('st-scanroot-save').addEventListener('click', () => save({ scanRoot: document.getElementById('st-scanroot').value.trim() || null }, t('Scan folder saved.')));
    document.getElementById('st-check').addEventListener('click', () => guard(async () => {
      const el = document.getElementById('st-check-result');
      el.innerHTML = `<span class="muted">${t('Checking…')}</span>`;
      const status = await api.archiveStatus();
      const r = await api.checkAvailability();
      el.innerHTML = `<div class="notice ${r.unavailable ? 'warn' : 'ok'}">${status.available ? t('Archive drive: available.') : t('Archive drive: NOT available.')}
        ${tn(r.total, '{available} of {n} game folder available.', '{available} of {n} game folders available.', { available: r.available })}${r.unavailable ? `<br>${t('Unavailable: {titles}', { titles: r.unavailableTitles.map(esc).join(', ') })}<br>${t('Their records are kept and marked as unavailable.')}` : ''}</div>`;
      refreshFacets();
    }));
    document.getElementById('st-backup').addEventListener('click', () => guard(async () => {
      const b = await api.createBackup();
      toast(t('Backup created: {name}', { name: b.fileName }));
      renderBackups();
    }));
    document.getElementById('st-auto').addEventListener('change', (e) => save({ autoBackup: e.target.checked }));
    document.getElementById('st-keep').addEventListener('change', (e) => save({ autoBackupKeep: Number(e.target.value) }));
    document.getElementById('st-backups').addEventListener('click', (e) => {
      const b = e.target.closest('[data-restore]');
      if (!b) return;
      guard(async () => {
        const ok = await confirmBox({
          title: t('Restore database'),
          html: t('Replace the current catalog with the backup from <b>{created}</b>?\n\nAll changes made after that moment will be replaced. A safety backup of the current state is created first, so this step can be undone.\nGame folders and files are not affected.', { created: esc(b.dataset.created) }),
          ok: t('Restore'),
          danger: true,
        });
        if (!ok) return;
        const rep = await api.restoreBackup(b.dataset.restore);
        toast(t('Database restored ({games}, {entries}).', { games: tn(rep.games, '{n} game', '{n} games'), entries: tn(rep.journalEntries, '{n} journal entry', '{n} journal entries') }));
        await refreshFacets();
        await settingsView.show();
      });
    });
    document.getElementById('st-wizard').addEventListener('click', () => openWizard(true));
    const keyInput = document.getElementById('st-moby-key');
    document.getElementById('st-moby-show').addEventListener('click', (e) => {
      const show = keyInput.type === 'password';
      keyInput.type = show ? 'text' : 'password';
      e.target.textContent = show ? t('Hide') : t('Show');
    });
    const saveKey = () => save({ mobygamesKey: keyInput.value.trim() }, () => (keyInput.value.trim() ? t('MobyGames API key saved.') : t('MobyGames API key removed.')));
    document.getElementById('st-moby-save').addEventListener('click', saveKey);
    keyInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); saveKey(); } });
    if (params.focus === 'st-moby-key') {
      document.getElementById('st-details').scrollIntoView({ block: 'center' });
      keyInput.focus();
    }
    document.getElementById('st-welcome').addEventListener('change', (e) => save({ showWelcome: e.target.checked },
      e.target.checked ? t('The welcome window will show at start.') : t('The welcome window will not show at start.')));
    document.getElementById('st-detect').addEventListener('click', () => guard(async () => {
      const r = await api.detectInstalls();
      document.getElementById('st-detect-result').innerHTML = `<div class="notice ok">${tn(r.detected + r.manual, '{n} game installed on {machine}: {auto} found automatically, {manual} located by you.', '{n} games installed on {machine}: {auto} found automatically, {manual} located by you.', { machine: esc(r.machine), auto: r.detected, manual: r.manual })}</div>`;
    }));
    renderBackups();
  },
};
