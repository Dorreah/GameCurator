// Archive Scanner: find game candidates in a folder, review, import selected.

import { api, listen, imgSrc } from './api.js';
import { esc, formatBytes, guard, toast, status, alertBox, categoryLabel } from './ui.js';
import { state } from './store.js';
import { refreshFacets } from './facets.js';
import { gogSection, showGogSection, offerForImported } from './gogdetails.js';
import { t, tn, tm } from './i18n.js';

// Kept at module level so results survive switching views.
let running = false;
let progress = null;
let result = null;
let rows = []; // { cand, selected, title, year, genre, art }
let error = null;

function visible() {
  return state.view === 'scanner' && document.getElementById('sc-root');
}

export function initScannerEvents() {
  listen('scan-progress', (p) => {
    progress = p;
    if (visible()) renderProgress();
  });
  listen('scan-finished', (payload) => {
    running = false;
    progress = null;
    if (payload.error) {
      error = tm(payload.error);
      result = null;
      rows = [];
    } else {
      error = null;
      result = payload.result;
      rows = result.candidates.map((c) => ({
        cand: c,
        selected: c.status === 'new' && c.confidence === 'strong',
        title: c.title,
        year: '',
        genre: '',
        art: null,
      }));
    }
    if (visible()) {
      render();
      // the results matter now: the guide steps aside (unless the user opened it on purpose)
      if (rows.length && helpPref() !== 'open') setHelp(false, false);
      if (rows.length) document.getElementById('sc-results')?.scrollIntoView({ block: 'start' });
    }
    if (payload.result) {
      const r = payload.result;
      toast(r.cancelled
        ? tn(r.candidates.length, 'Scan cancelled – {n} candidate found so far.', 'Scan cancelled – {n} candidates found so far.')
        : tn(r.candidates.length, 'Scan finished – {n} candidate found.', 'Scan finished – {n} candidates found.'), 'ok');
    } else {
      toast(t('Scan failed: {error}', { error: tm(payload.error) }), 'err');
    }
  });
}

function renderProgress() {
  const el = document.getElementById('sc-progress');
  if (!el) return;
  if (!running) { el.innerHTML = ''; return; }
  const n = progress ? progress.dirsScanned : 0;
  const lit = n % 25;
  el.innerHTML = `<div class="progress">${Array.from({ length: 25 }, (_, i) => `<i class="${i <= lit ? 'on' : ''}"></i>`).join('')}</div>
    <div class="scan-status"><span>${t('Folders scanned:')} <b>${n}</b></span><span>${t('Candidates:')} <b>${progress ? progress.candidates : 0}</b></span>
    <span class="scan-current mono">${esc(progress ? progress.current : t('Starting…'))}</span></div>`;
}

function statusCell(c) {
  if (c.status === 'in_catalog') return `<span class="badge known" title="${esc(c.existingTitle || '')}">${t('In catalog')}</span><div class="hint">${esc(c.existingTitle || '')}</div>`;
  if (c.status === 'possible_duplicate') return `<span class="badge dup">${t('Possible duplicate')}</span><div class="hint">${t('of {title}', { title: esc(c.existingTitle || '') })}</div>`;
  return `<span class="badge new">${t('New')}</span>${c.confidence === 'weak' ? `<div class="hint">${t('archives only')}</div>` : ''}`;
}

function summary(c) {
  const cats = c.categories.map((x) => `${x.count}× ${categoryLabel(x.category)}`).join(', ');
  return `${tn(c.fileCount, '{n} file', '{n} files')} · ${formatBytes(c.totalSize)}<br>${esc(cats)}${c.mainInstaller ? `<br>${t('Installer:')} <span class="mono">${esc(c.mainInstaller)}</span>` : ''}`;
}

function renderResults() {
  const el = document.getElementById('sc-results');
  if (!el) return;
  if (error) { el.innerHTML = `<div class="notice err">${esc(error)}</div>`; return; }
  if (!result) {
    el.innerHTML = `<div class="notice info">${t('Choose a folder (for example the folder with your GOG offline installers) and press <b>Start scan</b>. The scanner only reads the folders. Nothing is imported until you confirm it, and no file is ever run, moved or changed.')}</div>`;
    return;
  }
  if (!rows.length) {
    el.innerHTML = `<div class="notice warn">${result.cancelled ? t('No game candidates were found in {root} (scan cancelled).', { root: esc(result.root) }) : t('No game candidates were found in {root}.', { root: esc(result.root) })}
      <ul class="help-list">
        <li>${t('Raise the <b>search depth</b> if the games sit deeper (e.g. <span class="mono">Games\\RPG\\Baldur\'s Gate</span> needs 2 levels).')}</li>
        <li>${t('Choose the folder one level higher, or check that the game folders contain the installers (<span class="mono">.exe</span>, <span class="mono">.msi</span>, <span class="mono">.bin</span>) or archives (<span class="mono">.zip</span>, <span class="mono">.7z</span>, <span class="mono">.rar</span>).')}</li>
        <li>${t('A single game can always be added by hand with <b>Add Game</b> in the Library.')}</li>
      </ul></div>`;
    return;
  }
  const selectable = rows.filter((r) => r.cand.status !== 'in_catalog');
  const selected = rows.filter((r) => r.selected).length;
  el.innerHTML = `
    ${result.cancelled ? `<div class="notice warn">${t('The scan was cancelled; the list may be incomplete.')}</div>` : ''}
    <div class="row" style="margin-bottom:8px"><span>${tn(rows.length, '<b>{n}</b> candidate in {root}', '<b>{n}</b> candidates in {root}', { root: `<span class="mono">${esc(result.root)}</span>` })}</span>
      <span class="muted">(${tn(result.dirsScanned, '{n} folder', '{n} folders')}, ${(result.elapsedMs / 1000).toFixed(1)} s)</span></div>
    <table class="list"><thead><tr>
      <th><input type="checkbox" id="sc-all" ${selectable.length && selectable.every((r) => r.selected) ? 'checked' : ''} title="${t('Select all')}"></th>
      <th>${t('Title')}</th><th>${t('Year')}</th><th>${t('Genre')}</th><th>${t('Artwork')}</th><th>${t('Folder / contents')}</th><th>${t('Status')}</th></tr></thead>
    <tbody>${rows.map((r, i) => {
      const dis = r.cand.status === 'in_catalog';
      return `<tr data-row="${i}">
        <td><input type="checkbox" data-sel="${i}" ${r.selected ? 'checked' : ''} ${dis ? 'disabled' : ''}></td>
        <td><input class="field cand-title" data-title="${i}" value="${esc(r.title)}" ${dis ? 'disabled' : ''} maxlength="200"></td>
        <td><input class="field cand-small" data-year="${i}" type="number" min="1940" max="2100" value="${esc(r.year)}" ${dis ? 'disabled' : ''}></td>
        <td><input class="field cand-genre" data-genre="${i}" value="${esc(r.genre)}" ${dis ? 'disabled' : ''} autocomplete="off"></td>
        <td>${r.art ? `<img class="thumb-mini" src="${esc(imgSrc(r.art.thumbnailPath))}" alt=""> ` : ''}${dis ? '' : `<button class="btn small" data-art="${i}">${r.art ? t('Change') : t('Add…')}</button>`}</td>
        <td><div class="mono small">${esc(r.cand.folderPath)}</div><div class="cand-files">${summary(r.cand)}</div></td>
        <td>${statusCell(r.cand)}</td></tr>`;
    }).join('')}</tbody></table>

    <div class="row sticky-actions">
      <button class="btn gold" id="sc-import" ${selected ? '' : 'disabled'}>${t('Import selected ({n})', { n: selected })}</button>
      <span class="hint">${t('Title, year, genre and artwork can be filled in now or later in the game profile.')}</span></div>`;
}

function render() {
  const start = document.getElementById('sc-start');
  if (!start) return;
  start.disabled = running;
  document.getElementById('sc-cancel').disabled = !running;
  document.getElementById('sc-browse').disabled = running;
  renderProgress();
  renderResults();
}

async function startScan() {
  const root = document.getElementById('sc-root').value.trim();
  const depth = Number(document.getElementById('sc-depth').value);
  if (!root) { await alertBox(t('Please choose the folder to scan.'), { kind: 'warn' }); return; }
  // Mark as running *before* invoking: a small folder can finish (and emit
  // `scan-finished`) before the start command's response arrives.
  running = true;
  error = null;
  progress = null;
  render();
  const started = await guard(async () => {
    await api.startScan(root, depth);
    return true;
  });
  if (!started) {
    running = false;
    render();
    return;
  }
  // remember the folder for next time
  state.config = await api.saveConfig({ ...state.config, scanRoot: root, scanDepth: depth }).catch(() => state.config);
}

async function importSelected() {
  const items = rows.filter((r) => r.selected && r.cand.status !== 'in_catalog').map((r) => ({
    folderPath: r.cand.folderPath,
    title: r.title.trim() || r.cand.title,
    releaseYear: r.year ? Number(r.year) : null,
    genre: r.genre,
    artworkPath: r.art ? r.art.artworkPath : null,
    thumbnailPath: r.art ? r.art.thumbnailPath : null,
  }));
  if (!items.length) return;
  const btn = document.getElementById('sc-import');
  btn.disabled = true;
  btn.textContent = t('Importing…');
  const report = await guard(() => api.importCandidates(items));
  if (!report) { renderResults(); return; }
  const importedFolders = new Set(report.imported.map((g) => g.folderPath));
  for (const r of rows) {
    if (r.selected && importedFolders.has(r.cand.folderPath)) {
      r.cand.status = 'in_catalog';
      r.cand.existingTitle = r.title;
      r.selected = false;
    }
  }
  renderResults();
  await refreshFacets();
  const skipped = report.skipped.map((s) => `• ${s.title}: ${tm(s.reason)}`).join('\n');
  if (report.imported.length) offerForImported(report.imported.map((g) => g.id));
  const n = report.imported.length;
  await alertBox(`${tn(n, '{n} game imported.', '{n} games imported.')}${n ? `\n\n${t('Their details (description, genre, developer, cover\u2026) can be fetched from the internet on this page \u2013 from GOG.com, Databáze-her.cz or MobyGames.')}` : ''}${skipped ? `\n\n${t('Skipped:')}\n${skipped}` : ''}`, { title: t('Import finished'), kind: report.skipped.length ? 'warn' : 'info' });
}

const HELP_KEY = 'gc-scan-help';

/** 'open' or 'closed' when the user chose; '' = default (open until there are results). */
function helpPref() {
  try { return localStorage.getItem(HELP_KEY) || ''; } catch (_) { return ''; }
}

function setHelp(open, remember) {
  const box = document.getElementById('sc-help');
  const btn = document.getElementById('sc-help-btn');
  if (!box || !btn) return;
  box.hidden = !open;
  btn.setAttribute('aria-expanded', String(open));
  if (remember) {
    try { localStorage.setItem(HELP_KEY, open ? 'open' : 'closed'); } catch (_) { /* not remembered */ }
  }
}

/** Example path for the chosen search depth: Games\…\Game. */
function depthHint(depth) {
  const mid = Array.from({ length: Math.max(0, depth - 1) }, () => '…').join('\\');
  const example = `${t('Games')}\\${mid ? `${mid}\\` : ''}${t('Game')}`;
  return tn(depth, 'Looks {n} folder level deep: <span class="mono">{example}</span>', 'Looks up to {n} folder levels deep: <span class="mono">{example}</span>', { example: esc(example) });
}

/** How the scanner finds games: short, practical, open until the user hides it. */
function helpHtml() {
  return `<div class="help-box scan-help" id="sc-help">
    <div class="help-cols">
      <div>
        <b>${t('1. Choose the top folder')}</b>
        <p>${t('Pick the folder that holds your whole collection, e.g. <span class="mono">E:\\Games</span> or <span class="mono">GOG_Installers</span> – not the folder of one game. A drive letter alone works too, it just takes longer.')}</p>
        <b>${t('2. One game = one folder')}</b>
<pre class="help-tree">${esc(t('Games'))}\\
├─ Baldur's Gate\\    ${esc(t('← a game'))}
│  ├─ setup_bg.exe
│  ├─ setup_bg-1.bin
│  └─ Extras\\        ${esc(t('← part of the game'))}
└─ RPG\\              ${esc(t('← a group'))}
   └─ Planescape\\    ${esc(t('← a game (depth 2)'))}</pre>
      </div>
      <div>
        <b>${t('How a game is recognised')}</b>
        <ul>
          <li>${t('A folder with an installer directly inside (<span class="mono">.exe</span>, <span class="mono">.msi</span>, <span class="mono">.bin</span>) is one game; its subfolders (extras, soundtrack, patches) belong to it.')}</li>
          <li>${t('Installers in a subfolder such as <span class="mono">Installer</span>, <span class="mono">Setup</span>, <span class="mono">Win</span> or <span class="mono">Extras</span> count for the folder above.')}</li>
          <li>${t('A folder with only archives (<span class="mono">.zip</span>, <span class="mono">.7z</span>, <span class="mono">.rar</span>) is a weaker candidate and is not ticked in advance.')}</li>
          <li>${t('The title comes from the GOG file <span class="mono">goggame-….info</span>, a <span class="mono">title.txt</span>, the folder name or the installer name. You can change it before importing.')}</li>
        </ul>
      </div>
      <div>
        <b>${t('Search depth')}</b>
        <p>${t('How many folder levels below the chosen folder are searched. <b>1</b> = only <span class="mono">Games\\Game</span>; <b>2</b> = also <span class="mono">Games\\Group\\Game</span>; <b>3</b> (default) suits most collections.')}</p>
        <b>${t('After the scan')}</b>
        <ul>
          <li>${t('<b>New</b> – not in the catalog yet (ticked when installers were found).')}</li>
          <li>${t('<b>In catalog</b> – this folder (or the folder around it) is already a game in your catalog.')}</li>
          <li>${t('<b>Possible duplicate</b> – another game has the same title; check it before importing.')}</li>
        </ul>
        <p class="hint">${t('The scanner only reads. Nothing is imported until you confirm it, and no file is ever run, moved or changed. Folders on the archive drive are stored relative to Game Curator, so they keep working under another drive letter.')}</p>
      </div>
    </div></div>`;
}

export const scannerView = {
  async show(params = {}) {
    const cfg = state.config;
    const main = document.getElementById('main');
    const pref = helpPref();
    const helpOpen = pref === 'open' || (pref === '' && !rows.length);
    const depth = cfg.scanDepth || 3;
    main.innerHTML = `<div class="view">
      <div class="strip"><span>${t('Archive Scanner')}</span></div>
      <div class="library-scroll">
        <fieldset class="group"><legend>${t('Scan location')}</legend>
          <div class="row"><input class="field mono" id="sc-root" placeholder="${esc(t('Folder to scan'))}" value="${esc(cfg.scanRoot || (state.info && state.info.gamesDir) || '')}">
            <button class="btn" id="sc-browse">${t('Browse…')}</button>
            <label class="muted" for="sc-depth" style="margin-left:8px">${t('Search depth')}</label>
            <select class="field" id="sc-depth" style="flex:none;width:175px">
              ${[1, 2, 3, 4, 5].map((d) => `<option value="${d}" ${d === depth ? 'selected' : ''}>${tn(d, '{n} level', '{n} levels')}${d === 3 ? t(' (default)') : ''}</option>`).join('')}</select>
            <button class="btn gold" id="sc-start">${t('Start scan')}</button>
            <button class="btn" id="sc-cancel" disabled>${t('Cancel')}</button></div>
          <div class="row scan-under">
            <span class="hint" id="sc-depth-hint">${depthHint(depth)}</span>
            <span class="spacer"></span>
            <button class="btn small help-toggle" id="sc-help-btn" aria-expanded="${helpOpen}" aria-controls="sc-help">? ${t('How the scanner finds games')}</button>
          </div>
          ${helpHtml()}
          <div id="sc-progress" style="margin-top:10px"></div>
        </fieldset>
        ${gogSection()}
        <div id="sc-results"></div>
      </div></div>`;
    document.getElementById('sc-help').hidden = !helpOpen;
    document.getElementById('sc-help-btn').addEventListener('click', () => {
      setHelp(document.getElementById('sc-help').hidden, true);
    });
    document.getElementById('sc-depth').addEventListener('change', (e) => {
      document.getElementById('sc-depth-hint').innerHTML = depthHint(Number(e.target.value));
    });
    document.getElementById('sc-browse').addEventListener('click', () => guard(async () => {
      const p = await api.pickFolder(t('Choose the folder to scan'), document.getElementById('sc-root').value);
      if (p) document.getElementById('sc-root').value = p;
    }));
    document.getElementById('sc-start').addEventListener('click', startScan);
    document.getElementById('sc-cancel').addEventListener('click', () => guard(async () => {
      status(t('Cancelling the scan…'));
      await api.cancelScan();
    }));
    const results = document.getElementById('sc-results');
    results.addEventListener('change', (e) => {
      const el = e.target;
      if (el.id === 'sc-all') {
        rows.forEach((r) => { if (r.cand.status !== 'in_catalog') r.selected = el.checked; });
        renderResults();
      } else if (el.dataset.sel !== undefined) {
        rows[Number(el.dataset.sel)].selected = el.checked;
        renderResults();
      }
    });
    results.addEventListener('input', (e) => {
      const el = e.target;
      if (el.dataset.title !== undefined) rows[Number(el.dataset.title)].title = el.value;
      if (el.dataset.year !== undefined) rows[Number(el.dataset.year)].year = el.value;
      if (el.dataset.genre !== undefined) rows[Number(el.dataset.genre)].genre = el.value;
    });
    results.addEventListener('click', (e) => {
      const art = e.target.closest('[data-art]');
      if (art) {
        const r = rows[Number(art.dataset.art)];
        guard(async () => {
          const a = await api.pickArtwork();
          if (a) { r.art = a; renderResults(); }
        });
      }
      if (e.target.closest('#sc-import')) importSelected();
    });
    render();
    showGogSection();
    if (params.autoStart && !running) await startScan();
  },
};

