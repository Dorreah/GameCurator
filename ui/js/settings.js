// Settings: appearance, library display, archive, backups.

import { api } from './api.js';
import { esc, formatBytes, guard, toast, confirmBox } from './ui.js';
import { state, THEMES, CLASSIC_FONTS, applyTheme, applyClassicFont } from './store.js';
import { refreshFacets } from './facets.js';
import { openWizard } from './wizard.js';

async function save(patch, message = 'Settings saved.') {
  await guard(async () => {
    state.config = await api.saveConfig({ ...state.config, ...patch });
    toast(message);
  });
}

const SWATCH_TEXT = {
  classic: '<b>Game<br>Curator</b><span>Personal game archive</span>',
  amber: '<b><i>C:\\&gt;</i> GAME CURATOR<u>█</u></b><span>&gt; READY.</span>',
  matrix: '<b><i>&gt;</i> GAME CURATOR<u>█</u></b><span>&gt; wake up, curator…</span>',
  dos: '<b><i>A:\\&gt;</i> GAME CURATOR_</b><span>Alt+1 Library · Alt+5</span>',
};

function themePicker(current) {
  return `<div class="theme-picker" id="st-theme">${Object.entries(THEMES).map(([key, t]) => `
    <label class="theme-option" title="${esc(t.description)}">
      <input type="radio" name="theme" value="${key}" ${current === key ? 'checked' : ''}>
      <div class="theme-swatch ${key}">${SWATCH_TEXT[key]}</div>
      <span class="theme-name">${esc(t.name)}</span><span class="hint">${esc(t.description)}</span>
    </label>`).join('')}</div>`;
}

const FONT_SAMPLE = 'Game Curator – Příliš žluťoučký kůň';

function fontPicker(current) {
  return `<div class="font-picker" id="st-font">${Object.entries(CLASSIC_FONTS).map(([key, f]) => `
    <label class="font-option">
      <input type="radio" name="classicFont" value="${key}" ${current === key ? 'checked' : ''}>
      <span class="font-sample font-${key}">${esc(FONT_SAMPLE)}</span>
      <span class="font-name">${esc(f.name)}</span><span class="hint">${esc(f.note)}</span>
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
  if (!list.length) { el.innerHTML = '<div class="muted" style="padding:8px">No backups yet.</div>'; return; }
  const kind = { manual: 'Manual', auto: 'Automatic', 'pre-restore': 'Before restore', 'pre-upgrade': 'Before upgrade' };
  el.innerHTML = `<table class="list"><thead><tr><th>Created</th><th>Type</th><th class="num">Size</th><th></th></tr></thead><tbody>
    ${list.map((b) => `<tr><td>${esc(b.created)}</td><td>${esc(kind[b.kind] || b.kind)}</td><td class="num">${formatBytes(b.sizeBytes)}</td>
      <td class="act"><button class="btn small" data-restore="${esc(b.fileName)}" data-created="${esc(b.created)}">Restore…</button></td></tr>`).join('')}
    </tbody></table>`;
}

export const settingsView = {
  async show() {
    const cfg = state.config;
    const info = await api.appInfo().catch(() => state.info);
    state.info = info;
    const main = document.getElementById('main');
    main.innerHTML = `<div class="view"><div class="strip"><span>Settings</span></div>
    <div class="library-scroll"><div class="settings">
      <fieldset class="group"><legend>Appearance</legend>
        ${themePicker(cfg.theme)}
        <p class="hint" style="margin:10px 0 0">Themes change only the look. Layout, controls and keyboard shortcuts stay the same.</p>
        <div class="form-label" style="margin-top:16px">Font of the Classic look</div>
        ${fontPicker(cfg.classicFont || 'pixel-operator')}
        <p class="hint" id="st-font-note" style="margin:8px 0 0">${cfg.theme === 'classic' ? 'Used everywhere in the Classic look.' : 'Used when the Classic look is selected; the other themes keep their own font.'}</p>
        <label class="check" style="margin-top:12px"><input type="checkbox" id="st-welcome" ${cfg.showWelcome !== false ? 'checked' : ''}>
          Show the welcome window when Game Curator starts</label>
      </fieldset>

      <fieldset class="group"><legend>Gaming Journal</legend>
        <label class="check"><input type="checkbox" id="st-journal-prompt" ${cfg.journalPrompt !== false ? 'checked' : ''}>
          After playing, offer to write a journal entry</label>
        <div class="hint" style="margin-top:4px">When a game started with <b>▶ Play</b> ends, Game Curator asks whether you want to write down your impressions while they are fresh. It only notices games started from here, and never counts the time you play.</div>
      </fieldset>

      <fieldset class="group"><legend>Manuals &amp; music</legend>
        <label class="check"><input type="checkbox" id="st-viewer" ${cfg.builtinViewer !== false ? 'checked' : ''}>
          Open PDF manuals and music in Game Curator</label>
        <div class="hint" style="margin-top:4px"><b>Open</b> on a PDF shows it in a reader window, on a music file it plays the game's soundtrack in a small player &ndash; no other program needed. Both windows have <b>Open in default app</b>; switch this off to always use the programs set in Windows.</div>
      </fieldset>

      <fieldset class="group"><legend>Library</legend>
        <div class="form-grid">
          <label>Library view</label><div>${radio('viewMode', 'grid', cfg.viewMode, 'Grid View')}${radio('viewMode', 'catalog', cfg.viewMode, 'Catalog View')}</div>
          <label>Card size</label><div>${radio('cardSize', 'small', cfg.cardSize, 'Small')}${radio('cardSize', 'medium', cfg.cardSize, 'Medium')}${radio('cardSize', 'large', cfg.cardSize, 'Large')}</div>
        </div>
      </fieldset>

      <fieldset class="group"><legend>Archive</legend>
        <div class="form-grid">
          <label>Archive location</label><div><span class="led ${state.archiveOk ? 'on' : 'off'}"></span> <span class="mono">${esc(info.archiveRoot)}</span><div class="hint">${esc(info.driveKind)} · all data is stored in <span class="mono">Data</span> next to GameCurator.exe</div></div>
          <label for="st-scanroot">Scan folder</label>
          <div class="row"><input class="field mono" id="st-scanroot" value="${esc(cfg.scanRoot || '')}" placeholder="${esc(info.gamesDir)}"><button class="btn" id="st-scanroot-browse">Browse…</button><button class="btn" id="st-scanroot-save">Save</button></div>
          <label>Availability</label>
          <div><button class="btn" id="st-check">Check archive availability</button><div id="st-check-result" style="margin-top:8px"></div></div>
          <label>Installed games</label>
          <div><button class="btn" id="st-detect">Look for installed games again</button>
            <div class="hint">Game Curator looks at the GOG installation records and the Windows list of programs (read-only) each time it starts. Installations are remembered per computer (this one: <span class="mono">${esc(info.machine || '')}</span>).</div>
            <div id="st-detect-result" style="margin-top:8px"></div></div>
          <label>Setup</label><div><button class="btn" id="st-wizard">Run the setup wizard again</button></div>
        </div>
      </fieldset>

      <fieldset class="group"><legend>Database &amp; backups</legend>
        <div class="row" style="margin-bottom:10px;flex-wrap:wrap">
          <button class="btn gold" id="st-backup">Back up database now</button>
          <label class="check" style="margin-left:14px"><input type="checkbox" id="st-auto" ${cfg.autoBackup ? 'checked' : ''}> Automatic backup at start (at most once a day)</label>
          <label class="muted" for="st-keep" style="margin-left:10px">keep</label>
          <select class="field" id="st-keep" style="flex:none;width:80px">${[3, 5, 10, 20, 30].map((n) => `<option ${n === cfg.autoBackupKeep ? 'selected' : ''}>${n}</option>`).join('')}</select>
        </div>
        <div class="backup-list" id="st-backups"></div>
        <div class="hint" style="margin-top:6px">Backups are stored in <span class="mono">Data/Backups</span>. Restoring creates a safety backup of the current state first. Game folders are never affected.</div>
      </fieldset>

    </div></div></div>`;

    main.querySelectorAll('input[name=theme]').forEach((r) => r.addEventListener('change', () => {
      applyTheme(r.value);
      save({ theme: r.value }, `Theme: ${THEMES[r.value].name}`);
    }));
    main.querySelectorAll('input[name=classicFont]').forEach((r) => r.addEventListener('change', () => {
      applyClassicFont(r.value);
      save({ classicFont: r.value }, `Classic font: ${CLASSIC_FONTS[r.value].name}`);
    }));
    document.getElementById('st-journal-prompt').addEventListener('change', (e) => save({ journalPrompt: e.target.checked },
      e.target.checked ? 'Game Curator will offer a journal entry after playing.' : 'No journal offer after playing.'));
    document.getElementById('st-viewer').addEventListener('change', (e) => save({ builtinViewer: e.target.checked },
      e.target.checked ? 'PDFs and music open in Game Curator.' : 'PDFs and music open in the default applications.'));
    main.querySelectorAll('input[name=viewMode]').forEach((r) => r.addEventListener('change', () => save({ viewMode: r.value })));
    main.querySelectorAll('input[name=cardSize]').forEach((r) => r.addEventListener('change', () => save({ cardSize: r.value })));
    document.getElementById('st-scanroot-browse').addEventListener('click', () => guard(async () => {
      const p = await api.pickFolder('Choose the default folder for the Archive Scanner', document.getElementById('st-scanroot').value || info.gamesDir);
      if (p) { document.getElementById('st-scanroot').value = p; await save({ scanRoot: p }, 'Scan folder saved.'); }
    }));
    document.getElementById('st-scanroot-save').addEventListener('click', () => save({ scanRoot: document.getElementById('st-scanroot').value.trim() || null }, 'Scan folder saved.'));
    document.getElementById('st-check').addEventListener('click', () => guard(async () => {
      const el = document.getElementById('st-check-result');
      el.innerHTML = '<span class="muted">Checking…</span>';
      const status = await api.archiveStatus();
      const r = await api.checkAvailability();
      el.innerHTML = `<div class="notice ${r.unavailable ? 'warn' : 'ok'}">Archive drive: ${status.available ? 'available' : 'NOT available'}.
        ${r.available} of ${r.total} game folder(s) available.${r.unavailable ? `<br>Unavailable: ${r.unavailableTitles.map(esc).join(', ')}<br>Their records are kept and marked as unavailable.` : ''}</div>`;
      refreshFacets();
    }));
    document.getElementById('st-backup').addEventListener('click', () => guard(async () => {
      const b = await api.createBackup();
      toast(`Backup created: ${b.fileName}`);
      renderBackups();
    }));
    document.getElementById('st-auto').addEventListener('change', (e) => save({ autoBackup: e.target.checked }));
    document.getElementById('st-keep').addEventListener('change', (e) => save({ autoBackupKeep: Number(e.target.value) }));
    document.getElementById('st-backups').addEventListener('click', (e) => {
      const b = e.target.closest('[data-restore]');
      if (!b) return;
      guard(async () => {
        const ok = await confirmBox({
          title: 'Restore database',
          html: `Replace the current catalog with the backup from <b>${esc(b.dataset.created)}</b>?\n\nAll changes made after that moment will be replaced. A safety backup of the current state is created first, so this step can be undone.\nGame folders and files are not affected.`,
          ok: 'Restore',
          danger: true,
        });
        if (!ok) return;
        const rep = await api.restoreBackup(b.dataset.restore);
        toast(`Database restored (${rep.games} games, ${rep.journalEntries} journal entries).`);
        await refreshFacets();
        await settingsView.show();
      });
    });
    document.getElementById('st-wizard').addEventListener('click', () => openWizard(true));
    document.getElementById('st-welcome').addEventListener('change', (e) => save({ showWelcome: e.target.checked },
      e.target.checked ? 'The welcome window will show at start.' : 'The welcome window will not show at start.'));
    document.getElementById('st-detect').addEventListener('click', () => guard(async () => {
      const r = await api.detectInstalls();
      document.getElementById('st-detect-result').innerHTML = `<div class="notice ok">${r.detected + r.manual} game(s) installed on ${esc(r.machine)}: ${r.detected} found automatically, ${r.manual} located by you.</div>`;
    }));
    renderBackups();
  },
};
