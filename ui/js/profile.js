// Game profile: the game, its archive (files grouped by kind), what I know
// about it, and the journal.

import { api, imgSrc } from './api.js';
import {
  esc, initials, formatBytes, formatDate, guard, toast, confirmBox, openModal, installBox, CATEGORY_LABELS, CATEGORY_GROUPS, categoryLabel,
} from './ui.js';
import { t, tn } from './i18n.js';
import { createCropper } from './cropper.js';
import { shotsPanelHtml, openGallery, addShots, SHOT_DROP_HINT } from './gallery.js';
import { additionsPanelHtml, bindAdditions, addFiles, ADDITIONS_DROP_HINT } from './additions.js';
import { onFiles } from './dropzone.js';
import { navigate, setLeaveGuard, state, openHint } from './store.js';
import { refreshFacets } from './facets.js';
import { openGameForm } from './gameform.js';
import { openEntryEditor, deleteEntry, entryHtml } from './journal.js';

let game = null;
let install = null; // installation on this computer, or null
let shots = []; // screenshots, newest first
let extras = []; // additional files
let content = null;
let recent = [];
let notesDirty = false;

// a game started with Play has started or ended: update Play / Running
document.addEventListener('gc-running-changed', () => {
  const box = document.getElementById('pf-install');
  if (game && box) box.innerHTML = renderInstall();
});

// ---------------------------------------------------------------- game

function renderInfo() {
  const g = game;
  const cover = g.thumbnailPath
    ? `<img src="${esc(imgSrc(g.thumbnailPath))}" alt="">
       <button class="btn small adjust-art" id="pf-adjust" title="${t('Choose the part of the artwork shown on the card')}">${t('Adjust')}</button>`
    : `<div class="cover" style="position:absolute;inset:0"><div class="ph">${esc(initials(g.title))}<span></span></div></div>`;
  const facts = [
    [t('Developer'), esc(g.developer)],
    [t('Publisher'), esc(g.publisher)],
    [t('Released'), g.releaseYear ?? ''],
    [t('Genre'), esc(g.genre)],
    [t('Tags'), g.tags.map((tag) => `<span class="chip">${esc(tag)}</span>`).join('')],
  ].filter(([, v]) => v !== '' && v != null);
  return `
    <div class="game-info">
      <div class="cover-big">${cover}</div>
      <div>
        <dl class="facts">${facts.map(([k, v]) => `<dt>${k}</dt><dd>${v}</dd>`).join('')}
          <dt>${t('In archive')}</dt><dd>${t('since {date}', { date: formatDate(g.dateAdded) })}</dd></dl>
        ${facts.length < 3 ? `<p class="hint" style="margin:10px 0 0">${t('Developer, year, genre or tags can be added with “Edit”.')}</p>` : ''}
      </div>
    </div>
    <div class="install-row" id="pf-install">${renderInstall()}</div>
    ${g.description ? `<div class="desc">${esc(g.description)}</div>` : ''}`;
}

/** Installed on this PC? Where? */
function renderInstall() {
  const i = install;
  const SOURCE = { gog: t('found in the GOG installation records'), uninstall: t('found in the Windows list of programs'), manual: t('set by you') };
  if (!i) {
    return `${installBox(false)}<div class="inst-text"><b>${t('Not installed on this PC')}</b>
        <div class="hint">${t('If it is installed but was not found, point to its program.')}</div></div>
      <div class="inst-actions"><button class="btn small" id="pf-inst-locate">${t('Locate…')}</button></div>`;
  }
  // the path is not printed (less text on screen): "Show location" opens it,
  // and the tooltip tells where it is and how it was found
  const where = [i.installDir, i.exe ? t('Program: {exe}', { exe: i.exe }) : '', SOURCE[i.source] || i.source].filter(Boolean).join('\n');
  let note = '';
  if (!i.found) note = `<div class="warn-text">${t('The installation folder was not found.')}</div>`;
  else if (!i.canPlay) note = `<div class="warn-text">${t("The game's program is not known – use Locate…")}</div>`;
  return `${installBox(i.found)}<div class="inst-text" title="${esc(where)}"><b>${t('Installed on this PC')}</b>${note}</div>
      ${!i.canPlay ? '' : state.running.has(game.id)
    ? `<button class="btn gold inst-play" id="pf-play" disabled title="${t('The game is running. When it ends, Game Curator offers a journal entry.')}">● ${t('Running')}</button>`
    : `<button class="btn gold inst-play" id="pf-play" title="${t('Start the installed game (asks first)')}">▶ ${t('Play')}</button>`}
      <div class="inst-actions">
        ${i.found ? `<button class="btn small" id="pf-inst-folder" title="${esc(t('Open the installation folder in Explorer:'))}\n${esc(i.installDir)}">${t('Show location')}</button>` : ''}
        <button class="btn small" id="pf-inst-locate" title="${esc(t("Choose the game's program yourself"))}">${i.found && i.canPlay ? t('Change…') : t('Locate…')}</button>
        ${i.source === 'manual' || !i.found ? `<button class="btn small" id="pf-inst-forget" title="${t('Forget this location (nothing on disk is touched)')}">${t('Forget')}</button>` : ''}
      </div>`;
}

async function playFlow() {
  const ok = await confirmBox({
    title: t('Start game?'),
    html: t('<b>{title}</b> will be started:\n<span class="mono">{exe}</span>\n\nGame Curator never starts anything on its own.', { title: esc(game.title), exe: esc(install.exe) }),
    ok: `▶ ${t('Play')}`,
    cancel: t('Cancel'),
  });
  if (!ok) return;
  await api.playGame(game.id);
  toast(t('Started {name}.', { name: game.title }));
}

/** The part of the artwork shown on the card, chosen in a small window. */
async function adjustArtworkFlow() {
  const m = openModal({
    title: t('Artwork – {title}', { title: game.title }),
    width: 380,
    body: `<div id="adj-host" style="display:flex;justify-content:center"></div><p class="hint" style="margin:10px 0 0;text-align:center">${t('Drag to choose what the card shows. Zoom with the wheel or the slider; “Fit” shows the whole picture.')}</p>`,
    footer: `<button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="adj-save">${t('Save')}</button>`,
  });
  const cropper = createCropper(m.el.querySelector('#adj-host'), { src: imgSrc(game.artworkPath), crop: game.coverCrop, width: 270 });
  m.el.querySelector('#adj-save').addEventListener('click', () => m.close(cropper.getCrop()));
  const crop = await m.result;
  if (!crop) return;
  await api.setCoverCrop(game.id, crop);
  await reloadGame();
  render();
  toast(t('Artwork adjusted.'));
}

// ---------------------------------------------------------------- divider

const SPLIT_DEFAULT = 42.5; // percent for the left column (game, notes, journal)

/** The divider between the left column and the archive: drag, arrows, double-click. */
function bindSplit() {
  const grid = document.getElementById('pf-grid');
  const bar = document.getElementById('pf-split');
  if (!grid || !bar) return;
  const current = () => Number(grid.style.getPropertyValue('--split')) || SPLIT_DEFAULT;
  const set = (pct) => {
    const w = grid.clientWidth - 16; // padding
    // both columns keep their minimum width (320 px game, 380 px archive)
    const lo = Math.max(25, (320 / w) * 100);
    const hi = Math.min(75, ((w - 14 - 380) / w) * 100);
    const v = Math.round(Math.min(Math.max(pct, lo), Math.max(lo, hi)) * 10) / 10;
    grid.style.setProperty('--split', String(v));
    bar.setAttribute('aria-valuenow', String(Math.round(v)));
    return v;
  };
  const save = (v) => {
    if (!state.config || state.config.profileSplit === v) return;
    state.config.profileSplit = v;
    api.saveConfig({ ...state.config }).then((c) => { state.config = c; }).catch(() => {});
  };
  bar.setAttribute('aria-valuemin', '25');
  bar.setAttribute('aria-valuemax', '75');
  bar.setAttribute('aria-valuenow', String(Math.round(current())));
  let drag = false;
  bar.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    drag = true;
    try { bar.setPointerCapture(e.pointerId); } catch (_) { /* synthetic pointer */ }
    grid.classList.add('resizing');
    e.preventDefault();
  });
  bar.addEventListener('pointermove', (e) => {
    if (!drag) return;
    const r = grid.getBoundingClientRect();
    set(((e.clientX - r.left - 8 - 7) / (r.width - 16)) * 100);
  });
  const end = () => {
    if (!drag) return;
    drag = false;
    grid.classList.remove('resizing');
    save(current());
  };
  bar.addEventListener('pointerup', end);
  bar.addEventListener('pointercancel', end);
  bar.addEventListener('dblclick', () => save(set(SPLIT_DEFAULT)));
  bar.addEventListener('keydown', (e) => {
    const step = e.shiftKey ? 10 : 2;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      save(set(current() + (e.key === 'ArrowLeft' ? -step : step)));
    } else if (e.key === 'Home' || e.key === 'Enter') {
      e.preventDefault();
      save(set(SPLIT_DEFAULT));
    }
  });
}

// ---------------------------------------------------------------- archive

function categoryOptions(f) {
  const auto = `<option value="auto" ${f.customCategory ? '' : 'selected'}>${t('{category} (auto)', { category: categoryLabel(f.fileType) })}</option>`;
  return auto + Object.keys(CATEGORY_LABELS)
    .map((k) => `<option value="${k}" ${f.customCategory === k ? 'selected' : ''}>${categoryLabel(k)}</option>`).join('');
}

function fileActions(f) {
  if (f.isMissing) return `<span class="badge missing">${t('missing')}</span>`;
  const rename = `<button class="btn small icon" data-rename="${f.id}" title="${t('Rename in the catalog – the file on disk keeps its name')}">✎</button>`;
  if (f.runnable) return `<button class="btn small" data-run="${f.id}" title="${t('Start this program (asks first)')}">▶ ${t('Run')}</button>${rename}`;
  if (f.risky) return `<button class="btn small" disabled title="${t('Scripts and shortcuts are not opened for safety')}">${t('Open')}</button>${rename}`;
  return `<button class="btn small" data-open="${f.id}" title="${esc(openHint(f.filename))}">${t('Open')}</button>${rename}`;
}

/** Name shown in the catalog instead of the file name; the file keeps its name. */
async function renameFileFlow(f) {
  const m = openModal({
    title: t('Name in the catalog'),
    width: 480,
    body: `<div class="notice err" id="rn-error" hidden></div>
      <label class="field-label" for="rn-name">${t('Name shown in Game Curator')}</label>
      <input class="field" id="rn-name" maxlength="120" spellcheck="false" style="width:100%;margin-top:4px" value="${esc(f.displayName || f.filename)}">
      <p class="hint" style="margin:10px 0 0">${t('Only the name in the catalog changes. The file on disk keeps its own name:')}</p>
      <div class="mono muted">${esc(f.filePath)}</div>`,
    footer: `${f.displayName ? `<button class="btn left" id="rn-reset">${t('Use the file name')}</button>` : ''}<button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="rn-save">${t('Save')}</button>`,
    onKey: (e) => {
      if (e.key === 'Enter' && e.target.id === 'rn-name') { e.preventDefault(); m.el.querySelector('#rn-save').click(); return true; }
      return false;
    },
  });
  const q = (sel) => m.el.querySelector(sel);
  const save = async (name) => {
    try {
      await api.setFileDisplayName(game.id, f.id, name);
      m.close(true);
    } catch (e) {
      q('#rn-error').textContent = String(e);
      q('#rn-error').hidden = false;
    }
  };
  q('#rn-save').addEventListener('click', () => save(q('#rn-name').value));
  q('#rn-reset')?.addEventListener('click', () => save(''));
  setTimeout(() => q('#rn-name').select(), 0);
  if (!(await m.result)) return;
  await refreshContent();
  toast(t('Name in the catalog changed. The file itself was not renamed.'));
}

function summary(files) {
  const present = files.filter((f) => !f.isMissing);
  const total = present.reduce((a, f) => a + (f.sizeBytes || 0), 0);
  const parts = CATEGORY_GROUPS
    .map(([cat, , one, many]) => [present.filter((f) => f.category === cat).length, one, many])
    .filter(([n]) => n > 0)
    .map(([n, one, many]) => tn(n, one, many));
  return `<div class="archive-summary"><b>${tn(present.length, '{n} file', '{n} files')}</b> · ${formatBytes(total)}${parts.length ? ` <span class="muted">— ${esc(parts.join(' · '))}</span>` : ''}</div>`;
}

function fileRow(f) {
  return `<tr class="${f.isMissing ? 'missing' : ''}">
    <td class="pri">${f.runnable ? `<button class="primary-btn ${f.isPrimary ? 'on' : ''}" data-primary="${f.id}" title="${f.isPrimary ? t('Main installer (click to unmark)') : t('Mark as the main installer')}">★</button>` : ''}</td>
    ${f.displayName
      ? `<td class="fname renamed" title="${esc(`${f.displayName}\n${t('File on disk: {path}', { path: f.filePath })}`)}">${esc(f.displayName)}</td>`
      : `<td class="mono fname" title="${esc(f.filePath)}">${esc(f.filePath)}</td>`}
    <td class="cat"><select class="field cat-select" data-cat="${f.id}" title="${t('Category')}">${categoryOptions(f)}</select></td>
    <td class="num">${formatBytes(f.sizeBytes)}</td>
    <td class="act">${fileActions(f)}</td></tr>`;
}

function renderContent() {
  const c = content;
  if (!c) return `<div class="muted">${t('Reading the folder…')}</div>`;
  let html = `<div class="pathline"><span class="led ${c.folderAvailable ? 'on' : 'off'}" title="${c.folderAvailable ? t('Folder available') : t('Folder not available')}"></span>
      <span class="mono">${esc(c.folder)}</span>
      <button class="btn small" id="pf-open-folder" ${c.folderAvailable ? '' : 'disabled'}>${t('Open Folder')}</button>
      <button class="btn small icon" id="pf-refresh" title="${t('Read the folder again')}">↻</button></div>`;
  if (!game.folderPortable) {
    html += `<div class="notice info">${t('This folder is on another drive than the archive; its path will not follow the archive to another computer.')}</div>`;
  }
  if (!c.folderAvailable) {
    html += `<div class="notice warn">${t('The folder is not available right now (is the drive connected?). The records below are kept unchanged.')}</div>`;
  }
  if (!c.files.length) {
    return `${html}<div class="muted">${t('No files in this folder.')}</div>`;
  }
  html += summary(c.files);
  const dirs = c.dirs.filter((d) => d.depth <= 2).slice(0, 40);
  if (dirs.length) {
    html += `<div class="subfolders">${dirs.map((d) => `<span class="chip" data-dir="${esc(d.relPath)}" title="${t('Open in Explorer')}">▸ ${esc(d.relPath)}</span>`).join('')}</div>`;
  }
  html += '<table class="list files"><tbody>';
  for (const [cat, label] of CATEGORY_GROUPS) {
    const group = c.files.filter((f) => f.category === cat);
    if (!group.length) continue;
    const size = group.filter((f) => !f.isMissing).reduce((a, f) => a + (f.sizeBytes || 0), 0);
    html += `<tr class="group"><th colspan="5">${esc(t(label))} <span class="n">${group.length} · ${formatBytes(size)}</span></th></tr>`;
    html += group.map(fileRow).join('');
  }
  html += '</tbody></table>';
  if (c.truncated) html += `<div class="notice info" style="margin-top:8px">${t('This folder is very large; only part of it is listed.')}</div>`;
  return html;
}

// ---------------------------------------------------------------- journal

function renderJournal() {
  if (!recent.length) {
    return `<p class="hint" style="margin:0">${t('Impressions, analysis, things you found out while playing or reading about the game – the journal keeps them with the archive.')}</p>`;
  }
  return `<div class="entries">${recent.map((e) => entryHtml(e, { compact: true, showGame: false })).join('')}</div>`;
}

// ---------------------------------------------------------------- page

function render() {
  const g = game;
  const main = document.getElementById('main');
  main.innerHTML = `<div class="view">
    <div class="profile-head">
      <button class="btn" id="pf-back">◄ ${t('Library')}</button>
      <h1 class="game-title${install ? ' inst' : ''}" title="${esc(g.title)}">${esc(g.title)}</h1>
      ${g.isAvailable ? '' : '<span class="badge offline">OFFLINE</span>'}
      <button class="btn" id="pf-edit">✎ ${t('Edit')}</button>
      <button class="btn danger" id="pf-remove">${t('Remove from catalog')}</button>
    </div>
    <div class="profile-grid" id="pf-grid" style="--split:${Number(state.config?.profileSplit) || SPLIT_DEFAULT}">
      <div class="pf-split" id="pf-split" role="separator" aria-orientation="vertical" aria-label="${t('Divider between the game and its archive')}"
        tabindex="0" title="${t('Drag to change the width of the panels · double-click to reset')}"><span></span></div>
      <section class="panel bevel-out pf-a"><div class="strip">${t('Game')}</div>
        <div class="panel-body" id="pf-info">${renderInfo()}</div></section>
      <div class="pf-right">
        <section class="panel bevel-out pf-b"><div class="strip">${t('Archive')}<span class="spacer"></span><span class="count" id="pf-file-count"></span></div>
          <div class="panel-body" id="pf-content">${renderContent()}</div></section>
        <section class="panel bevel-out pf-f"><div class="strip">${t('Additional files')}<span class="spacer"></span><span class="count" id="pf-add-count">${extras.length || ''}</span></div>
          <div class="panel-body" id="pf-additions">${additionsPanelHtml(extras)}</div></section>
      </div>
      <section class="panel bevel-out pf-d"><div class="strip">${t('What I know about this game')}</div>
        <div class="panel-body notes-body">
          <textarea class="field" id="pf-notes" maxlength="50000" placeholder="${esc(t('Where this copy comes from, which edition and installer version it is, what the extras contain, facts and stories you found out…'))}">${esc(g.archiveNotes)}</textarea>
          <div class="row"><button class="btn small" id="pf-notes-save" disabled>${t('Save')}</button><span class="hint" id="pf-notes-state">${t('Saved automatically when you leave the field.')}</span></div>
        </div></section>
      <section class="panel bevel-out pf-c"><div class="strip">${t('Gaming Journal')}<span class="spacer"></span><span class="count">${g.journalCount || ''}</span></div>
        <div class="panel-body"><div class="row" style="margin-bottom:10px"><button class="btn" id="pf-new-entry">＋ ${t('New entry')}</button>
          ${g.journalCount > recent.length ? `<button class="btn small" id="pf-all-entries">${t('All {n} entries in the Journal', { n: g.journalCount })}</button>` : ''}</div>
          <div id="pf-journal">${renderJournal()}</div></div></section>
      <section class="panel bevel-out pf-e"><div class="strip">${t('Screenshots')}<span class="spacer"></span><span class="count" id="pf-shot-count">${shots.length || ''}</span></div>
        <div class="panel-body" id="pf-shots">${shotsPanelHtml(shots, g.id)}</div></section>
    </div></div>`;
  updateFileCount();
  bind();
}

function updateFileCount() {
  const el = document.getElementById('pf-file-count');
  if (el && content) el.textContent = tn(content.files.filter((f) => !f.isMissing).length, '{n} file', '{n} files');
}

async function reloadGame() {
  game = await api.getGame(game.id);
}

async function saveNotes() {
  if (!notesDirty) return;
  const ta = document.getElementById('pf-notes');
  if (!ta) return;
  const value = ta.value;
  try {
    await api.patchGame(game.id, { archiveNotes: value });
    game.archiveNotes = value;
    notesDirty = false;
    const st = document.getElementById('pf-notes-state');
    if (st) st.textContent = t('Saved.');
    const b = document.getElementById('pf-notes-save');
    if (b) b.disabled = true;
  } catch (e) {
    toast(t('Notes were not saved: {error}', { error: e }), 'err');
    throw e;
  }
}

async function refreshContent() {
  content = await api.archiveContent(game.id);
  const el = document.getElementById('pf-content');
  if (el) el.innerHTML = renderContent();
  updateFileCount();
}

async function refreshShots() {
  shots = await api.listScreenshots(game.id);
  const el = document.getElementById('pf-shots');
  if (el) el.innerHTML = shotsPanelHtml(shots, game.id);
  const c = document.getElementById('pf-shot-count');
  if (c) c.textContent = shots.length || '';
}

async function refreshAdditions() {
  extras = await api.listAdditions(game.id);
  const el = document.getElementById('pf-additions');
  if (el) el.innerHTML = additionsPanelHtml(extras);
  const c = document.getElementById('pf-add-count');
  if (c) c.textContent = extras.length || '';
}

/** The gallery window; afterwards the strip and the journal show the changes. */
async function galleryFlow(startId = null) {
  let changed = false;
  await openGallery({ gameId: game.id, title: game.title, startId, onChange: () => { changed = true; } });
  if (changed) {
    await refreshShots();
    recent = await api.listJournal({ gameId: game.id, limit: 3 });
    const j = document.getElementById('pf-journal');
    if (j) j.innerHTML = renderJournal();
  }
}

async function refreshJournal() {
  [recent, shots] = await Promise.all([api.listJournal({ gameId: game.id, limit: 3 }), api.listScreenshots(game.id)]);
  await saveNotes().catch(() => {});
  await reloadGame();
  render();
}

async function removeFlow() {
  const p = await api.removalPreview(game.id);
  const m = openModal({
    title: t('Remove from catalog'),
    width: 540,
    body: `<div class="dlg-icon warn">!</div><div class="dlg-text">${t('Remove <b>{title}</b> from the catalog?', { title: esc(p.title) })}

<b>${t('The game folder and its files will NOT be deleted, moved or changed:')}</b>
<span class="mono">${esc(p.folder)}</span>

${p.journalEntries || p.fileRecords ? t('Also removed from the database: {entries} and {records}.', { entries: tn(p.journalEntries, '{n} journal entry', '{n} journal entries'), records: tn(p.fileRecords, '{n} file record', '{n} file records') }) : ''}</div>
      ${p.journalEntries ? `<label class="check" style="margin-top:12px"><input type="checkbox" id="rm-keep" checked> ${t('Keep readable copies of the journal entries in <span class="mono">Data/Journals</span>')}</label>` : ''}
      ${p.screenshots ? `<label class="check" style="margin-top:8px"><input type="checkbox" id="rm-keep-shots" checked> ${tn(p.screenshots, 'Keep the screenshot in <span class="mono">Data/Screenshots</span>', 'Keep the {n} screenshots in <span class="mono">Data/Screenshots</span>')}</label>` : ''}
      ${p.additionCopies ? `<label class="check" style="margin-top:8px"><input type="checkbox" id="rm-keep-adds" checked> ${tn(p.additionCopies, 'Keep the copied additional file in <span class="mono">Data/Additions</span>', 'Keep the {n} copied additional files in <span class="mono">Data/Additions</span>')}</label>` : ''}`,
    footer: `<button class="btn" data-close>${t('Cancel')}</button><button class="btn danger" id="rm-ok">${t('Remove from catalog')}</button>`,
  });
  m.el.querySelector('#rm-ok').addEventListener('click', () => {
    const checked = (sel) => { const el = m.el.querySelector(sel); return el ? el.checked : true; };
    m.close({ keep: checked('#rm-keep'), shots: checked('#rm-keep-shots'), adds: checked('#rm-keep-adds') });
  });
  setTimeout(() => m.el.querySelector('[data-close]').focus(), 0);
  const choice = await m.result;
  if (!choice) return;
  const result = await api.removeGame(game.id, choice.keep, choice.shots, choice.adds);
  toast(t('“{title}” was removed from the catalog. Its folder was not touched.', { title: result.title }));
  await refreshFacets();
  navigate('library');
}

function bind() {
  const id = game.id;
  const byId = (x) => document.getElementById(x);
  byId('pf-back').addEventListener('click', () => navigate('library'));
  byId('pf-edit').addEventListener('click', () => guard(async () => {
    await saveNotes();
    const saved = await openGameForm(game);
    if (saved) {
      await reloadGame();
      content = await api.archiveContent(id);
      render();
      refreshFacets();
    }
  }));
  byId('pf-remove').addEventListener('click', () => guard(async () => { await saveNotes(); await removeFlow(); }));
  bindSplit();
  byId('pf-adjust')?.addEventListener('click', () => guard(adjustArtworkFlow));

  // installed on this PC
  byId('pf-install').addEventListener('click', (e) => {
    const tg = e.target;
    const refresh = async () => { install = await api.installStatus(id); render(); };
    if (tg.closest('#pf-play')) return guard(playFlow);
    if (tg.closest('#pf-inst-folder')) return guard(() => api.openInstallFolder(id).then(() => toast(t('Installation folder opened.'))));
    if (tg.closest('#pf-inst-locate')) {
      return guard(async () => {
        const st = await api.locateInstall(id);
        if (st) { await refresh(); toast(st.canPlay ? t('Location saved. The game can be started with Play.') : t('Location saved.')); }
      });
    }
    if (tg.closest('#pf-inst-forget')) {
      return guard(async () => {
        const ok = await confirmBox({
          title: t('Forget the location?'),
          html: t('Game Curator will forget where <b>{title}</b> is installed on this PC.\nNothing on the disk is changed or deleted.', { title: esc(game.title) }),
          ok: t('Forget'),
        });
        if (!ok) return;
        await api.forgetInstall(id);
        await refresh();
      });
    }
    return undefined;
  });

  // archive
  const contentEl = byId('pf-content');
  contentEl.addEventListener('click', (e) => {
    const tg = e.target;
    if (tg.closest('#pf-open-folder')) return guard(() => api.openGameFolder(id, null).then(() => toast(t('Folder opened.'))));
    if (tg.closest('#pf-refresh')) return guard(async () => { await refreshContent(); toast(t('Folder contents refreshed.')); });
    const dir = tg.closest('[data-dir]');
    if (dir) return guard(() => api.openGameFolder(id, dir.dataset.dir));
    const open = tg.closest('[data-open]');
    if (open) return guard(() => api.openFile(id, Number(open.dataset.open)).then((how) => { if (how === 'external') toast(t('Opened with the default application.')); }));
    const run = tg.closest('[data-run]');
    if (run) {
      const f = content.files.find((x) => x.id === Number(run.dataset.run));
      return guard(async () => {
        const ok = await confirmBox({
          title: t('Run program?'),
          html: t('You are about to run:\n<span class="mono">{path}</span>\n\nPrograms and installers can make changes to your computer. Only run files you trust.\nGame Curator never runs anything on its own.', { path: esc(f.filePath) }),
          ok: t('Run'),
          cancel: t('Cancel'),
        });
        if (!ok) return;
        await api.runFile(id, f.id);
        toast(t('Started {name}.', { name: f.filename }));
      });
    }
    const ren = tg.closest('[data-rename]');
    if (ren) {
      const f = content.files.find((x) => x.id === Number(ren.dataset.rename));
      return guard(() => renameFileFlow(f));
    }
    const prim = tg.closest('[data-primary]');
    if (prim) {
      const fid = Number(prim.dataset.primary);
      const f = content.files.find((x) => x.id === fid);
      return guard(async () => {
        await api.setPrimaryFile(id, f.isPrimary ? null : fid);
        await refreshContent();
      });
    }
    return undefined;
  });
  contentEl.addEventListener('change', (e) => {
    const sel = e.target.closest('[data-cat]');
    if (!sel) return;
    guard(async () => {
      await api.setFileCategory(id, Number(sel.dataset.cat), sel.value === 'auto' ? null : sel.value);
      await refreshContent();
    });
  });

  // journal
  byId('pf-new-entry').addEventListener('click', () => guard(async () => {
    const saved = await openEntryEditor({ gameId: id });
    if (saved) await refreshJournal();
  }));
  byId('pf-all-entries')?.addEventListener('click', () => navigate('journal', { gameId: id }));

  // screenshots
  byId('pf-shots').addEventListener('click', (e) => {
    const tg = e.target;
    if (tg.closest('#pf-add-shot')) {
      return guard(async () => {
        const added = await addShots(id);
        if (added.length) await refreshShots();
      });
    }
    if (tg.closest('#pf-all-shots') || tg.closest('[data-shot-all]')) return guard(() => galleryFlow());
    const s = tg.closest('[data-shot]');
    if (s) return guard(() => galleryFlow(Number(s.dataset.shot)));
    return undefined;
  });

  // additional files
  bindAdditions(byId('pf-additions'), game, () => extras, refreshAdditions);

  // files dragged from Explorer
  onFiles(document.querySelector('.pf-e'), t(SHOT_DROP_HINT), (files) => guard(async () => {
    const added = await addShots(id, null, files);
    if (added.length) await refreshShots();
  }));
  onFiles(document.querySelector('.pf-f'), t(ADDITIONS_DROP_HINT), (files) => guard(async () => {
    if (await addFiles(game, files)) await refreshAdditions();
  }));
  byId('pf-journal').addEventListener('click', (e) => {
    const shot = e.target.closest('[data-shot]');
    if (shot) return guard(() => galleryFlow(Number(shot.dataset.shot)));
    if (e.target.closest('[data-shot-all]')) return guard(() => galleryFlow());
    const ed = e.target.closest('[data-edit-entry]');
    const del = e.target.closest('[data-delete-entry]');
    if (ed) {
      const entry = recent.find((x) => x.id === Number(ed.dataset.editEntry));
      guard(async () => { if (await openEntryEditor({ entry })) await refreshJournal(); });
    } else if (del) {
      const entry = recent.find((x) => x.id === Number(del.dataset.deleteEntry));
      guard(async () => { if (await deleteEntry(entry)) await refreshJournal(); });
    }
  });

  // what I know
  const notes = byId('pf-notes');
  notes.addEventListener('input', () => {
    notesDirty = notes.value !== game.archiveNotes;
    byId('pf-notes-save').disabled = !notesDirty;
    byId('pf-notes-state').textContent = notesDirty ? t('Unsaved changes.') : t('Saved.');
  });
  notes.addEventListener('change', () => saveNotes().catch(() => {}));
  notes.addEventListener('keydown', (e) => {
    if (e.key === 's' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); saveNotes().then(() => toast(t('Notes saved.'))).catch(() => {}); }
  });
  byId('pf-notes-save').addEventListener('click', () => saveNotes().then(() => toast(t('Notes saved.'))).catch(() => {}));
}

export const profileView = {
  async show({ id }) {
    const main = document.getElementById('main');
    main.innerHTML = `<div class="panel-body muted">${t('Loading…')}</div>`;
    notesDirty = false;
    try {
      [game, recent, install, shots, extras] = await Promise.all([
        api.getGame(id), api.listJournal({ gameId: id, limit: 3 }), api.installStatus(id).catch(() => null),
        api.listScreenshots(id), api.listAdditions(id),
      ]);
    } catch (e) {
      main.innerHTML = `<div class="panel-body"><div class="notice err">${esc(e)}</div><button class="btn" id="pf-back2">◄ ${t('Library')}</button></div>`;
      document.getElementById('pf-back2').addEventListener('click', () => navigate('library'));
      return;
    }
    content = null;
    render();
    setLeaveGuard(() => saveNotes());
    try {
      await refreshContent();
    } catch (e) {
      const el = document.getElementById('pf-content');
      if (el) el.innerHTML = `<div class="notice err">${esc(e)}</div>`;
    }
  },
};
