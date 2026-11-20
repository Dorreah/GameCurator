// Archive Scanner: find game candidates in a folder, review, import selected.

import { api, listen, imgSrc } from './api.js';
import { esc, formatBytes, guard, toast, status, alertBox, CATEGORY_LABELS } from './ui.js';
import { state } from './store.js';
import { refreshFacets } from './facets.js';

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
      error = payload.error;
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
    if (visible()) render();
    if (payload.result) {
      const r = payload.result;
      toast(r.cancelled ? `Scan cancelled – ${r.candidates.length} candidate(s) found so far.` : `Scan finished – ${r.candidates.length} candidate(s) found.`, 'ok');
    } else {
      toast(`Scan failed: ${payload.error}`, 'err');
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
    <div class="scan-status"><span>Folders scanned: <b>${n}</b></span><span>Candidates: <b>${progress ? progress.candidates : 0}</b></span>
    <span class="scan-current mono">${esc(progress ? progress.current : 'Starting…')}</span></div>`;
}

function statusCell(c) {
  if (c.status === 'in_catalog') return `<span class="badge known" title="${esc(c.existingTitle || '')}">In catalog</span><div class="hint">${esc(c.existingTitle || '')}</div>`;
  if (c.status === 'possible_duplicate') return `<span class="badge dup">Possible duplicate</span><div class="hint">of ${esc(c.existingTitle || '')}</div>`;
  return `<span class="badge new">New</span>${c.confidence === 'weak' ? '<div class="hint">archives only</div>' : ''}`;
}

function summary(c) {
  const cats = c.categories.map((x) => `${x.count}× ${CATEGORY_LABELS[x.category] || x.category}`).join(', ');
  return `${c.fileCount} files · ${formatBytes(c.totalSize)}<br>${esc(cats)}${c.mainInstaller ? `<br>Installer: <span class="mono">${esc(c.mainInstaller)}</span>` : ''}`;
}

function renderResults() {
  const el = document.getElementById('sc-results');
  if (!el) return;
  if (error) { el.innerHTML = `<div class="notice err">${esc(error)}</div>`; return; }
  if (!result) {
    el.innerHTML = `<div class="notice info">Choose a folder (for example the folder with your GOG offline installers) and press <b>Start scan</b>.
      The scanner only reads the folders. Nothing is imported until you confirm it, and no file is ever run, moved or changed.</div>`;
    return;
  }
  if (!rows.length) {
    el.innerHTML = `<div class="notice warn">No game candidates were found in ${esc(result.root)}${result.cancelled ? ' (scan cancelled)' : ''}. Try a deeper search level or a different folder.</div>`;
    return;
  }
  const selectable = rows.filter((r) => r.cand.status !== 'in_catalog');
  const selected = rows.filter((r) => r.selected).length;
  el.innerHTML = `
    ${result.cancelled ? '<div class="notice warn">The scan was cancelled; the list may be incomplete.</div>' : ''}
    <div class="row" style="margin-bottom:8px"><b>${rows.length}</b>&nbsp;candidate(s) in <span class="mono">${esc(result.root)}</span>
      <span class="muted">(${result.dirsScanned} folders, ${(result.elapsedMs / 1000).toFixed(1)} s)</span></div>
    <table class="list"><thead><tr>
      <th><input type="checkbox" id="sc-all" ${selectable.length && selectable.every((r) => r.selected) ? 'checked' : ''} title="Select all"></th>
      <th>Title</th><th>Year</th><th>Genre</th><th>Artwork</th><th>Folder / contents</th><th>Status</th></tr></thead>
    <tbody>${rows.map((r, i) => {
      const dis = r.cand.status === 'in_catalog';
      return `<tr data-row="${i}">
        <td><input type="checkbox" data-sel="${i}" ${r.selected ? 'checked' : ''} ${dis ? 'disabled' : ''}></td>
        <td><input class="field cand-title" data-title="${i}" value="${esc(r.title)}" ${dis ? 'disabled' : ''} maxlength="200"></td>
        <td><input class="field cand-small" data-year="${i}" type="number" min="1940" max="2100" value="${esc(r.year)}" ${dis ? 'disabled' : ''}></td>
        <td><input class="field cand-genre" data-genre="${i}" value="${esc(r.genre)}" ${dis ? 'disabled' : ''} list="sc-genres"></td>
        <td>${r.art ? `<img class="thumb-mini" src="${esc(imgSrc(r.art.thumbnailPath))}" alt=""> ` : ''}${dis ? '' : `<button class="btn small" data-art="${i}">${r.art ? 'Change' : 'Add…'}</button>`}</td>
        <td><div class="mono small">${esc(r.cand.folderPath)}</div><div class="cand-files">${summary(r.cand)}</div></td>
        <td>${statusCell(r.cand)}</td></tr>`;
    }).join('')}</tbody></table>
    <datalist id="sc-genres">${(state.facets ? state.facets.genres : []).map((g) => `<option value="${esc(g.name)}">`).join('')}</datalist>
    <div class="row sticky-actions">
      <button class="btn gold" id="sc-import" ${selected ? '' : 'disabled'}>Import selected (${selected})</button>
      <span class="hint">Title, year, genre and artwork can be filled in now or later in the game profile.</span></div>`;
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
  if (!root) { await alertBox('Please choose the folder to scan.', { kind: 'warn' }); return; }
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
  btn.textContent = 'Importing…';
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
  const skipped = report.skipped.map((s) => `• ${s.title}: ${s.reason}`).join('\n');
  await alertBox(`${report.imported.length} game(s) imported.${skipped ? `\n\nSkipped:\n${skipped}` : ''}`, { title: 'Import finished', kind: report.skipped.length ? 'warn' : 'info' });
}

export const scannerView = {
  async show(params = {}) {
    const cfg = state.config;
    const main = document.getElementById('main');
    main.innerHTML = `<div class="view">
      <div class="strip"><span>Archive Scanner</span></div>
      <div class="library-scroll">
        <fieldset class="group"><legend>Scan location</legend>
          <div class="row"><input class="field mono" id="sc-root" placeholder="Folder to scan" value="${esc(cfg.scanRoot || (state.info && state.info.gamesDir) || '')}">
            <button class="btn" id="sc-browse">Browse…</button>
            <label class="muted" for="sc-depth" style="margin-left:8px">Search depth</label>
            <select class="field" id="sc-depth" style="flex:none;width:175px">
              ${[1, 2, 3, 4, 5].map((d) => `<option value="${d}" ${d === (cfg.scanDepth || 3) ? 'selected' : ''}>${d} level${d > 1 ? 's' : ''}${d === 3 ? ' (default)' : ''}</option>`).join('')}</select>
            <button class="btn gold" id="sc-start">Start scan</button>
            <button class="btn" id="sc-cancel" disabled>Cancel</button></div>
          <div id="sc-progress" style="margin-top:10px"></div>
        </fieldset>
        <div id="sc-results"></div>
      </div></div>`;
    document.getElementById('sc-browse').addEventListener('click', () => guard(async () => {
      const p = await api.pickFolder('Choose the folder to scan', document.getElementById('sc-root').value);
      if (p) document.getElementById('sc-root').value = p;
    }));
    document.getElementById('sc-start').addEventListener('click', startScan);
    document.getElementById('sc-cancel').addEventListener('click', () => guard(async () => {
      status('Cancelling the scan…');
      await api.cancelScan();
    }));
    const results = document.getElementById('sc-results');
    results.addEventListener('change', (e) => {
      const t = e.target;
      if (t.id === 'sc-all') {
        rows.forEach((r) => { if (r.cand.status !== 'in_catalog') r.selected = t.checked; });
        renderResults();
      } else if (t.dataset.sel !== undefined) {
        rows[Number(t.dataset.sel)].selected = t.checked;
        renderResults();
      }
    });
    results.addEventListener('input', (e) => {
      const t = e.target;
      if (t.dataset.title !== undefined) rows[Number(t.dataset.title)].title = t.value;
      if (t.dataset.year !== undefined) rows[Number(t.dataset.year)].year = t.value;
      if (t.dataset.genre !== undefined) rows[Number(t.dataset.genre)].genre = t.value;
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
    if (params.autoStart && !running) await startScan();
  },
};

