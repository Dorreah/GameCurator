// Gaming Journal: entries are written in each game's profile; this page
// brings all of them together, grouped by game. Also the entry editor.

import { api, imgSrc } from './api.js';
import { esc, formatDate, todayIso, guard, toast, confirmBox, openModal } from './ui.js';
import { navigate } from './store.js';
import { createEditor, sanitizeHtml, plainText } from './editor.js';
import { shotStrip, addShots, openGallery, SHOT_DROP_HINT } from './gallery.js';
import { onFiles } from './dropzone.js';
import { t, tn } from './i18n.js';

/** HTML of one entry. Content is sanitized again at render time. */
export function entryHtml(e, { compact = false, showGame = true } = {}) {
  return `<article class="entry${compact ? ' compact' : ''}" data-entry="${e.id}">
    <div class="entry-head">
      <span class="entry-date">${esc(formatDate(e.entryDate))}</span>
      <span class="entry-title">${esc(e.title || t('(untitled)'))}</span>
      ${showGame ? `<button class="entry-game" data-game="${e.gameId}">${esc(e.gameTitle)}</button>` : ''}
      <button class="btn small" data-edit-entry="${e.id}">${t('Edit')}</button>
      <button class="btn small" data-delete-entry="${e.id}" title="${t('Delete entry')}">✕</button>
    </div>
    <div class="entry-body rich">${compact ? esc(plainText(e.content)) || `<span class="muted">${t('(no text)')}</span>` : sanitizeHtml(e.content) || `<p class="muted">${t('(no text)')}</p>`}</div>
    ${e.screenshots && e.screenshots.length ? `<div class="entry-shots">${shotStrip(e.screenshots, { max: compact ? 4 : 8, gameId: e.gameId })}</div>` : ''}
  </article>`;
}

export async function deleteEntry(entry) {
  const ok = await confirmBox({
    title: t('Delete journal entry'),
    html: t('Delete the entry <b>{title}</b> from {date}?\n\nThis cannot be undone (database backups are kept in Data/Backups).', { title: esc(entry.title || t('(untitled)')), date: esc(formatDate(entry.entryDate)) }),
    ok: t('Delete'),
    danger: true,
  });
  if (!ok) return false;
  await api.deleteJournal(entry.id);
  toast(t('Journal entry deleted.'));
  return true;
}

/**
 * Opens the editor for a new entry (`gameId`) or an existing `entry`.
 * Resolves to the saved entry or null.
 */
export async function openEntryEditor({ entry = null, gameId = null } = {}) {
  const titles = await api.gameTitles();
  if (!titles.length) {
    toast(t('Add a game first - every journal entry belongs to a game.'), 'err');
    return null;
  }
  const selected = entry ? entry.gameId : gameId;
  const m = openModal({
    title: entry ? t('Edit journal entry') : t('New journal entry'),
    width: 780,
    body: `<div class="notice err" id="je-error" hidden></div>
      <div class="form-grid" style="grid-template-columns:90px 1fr">
        <label class="req" for="je-game">${t('Game')}</label>
        <select class="field" id="je-game">${selected ? '' : `<option value="">${t('– choose a game –')}</option>`}
          ${titles.map((x) => `<option value="${x.id}" ${x.id === selected ? 'selected' : ''}>${esc(x.title)}</option>`).join('')}</select>
        <label class="req" for="je-date">${t('Date')}</label>
        <div><input class="field" id="je-date" type="date" style="width:170px" value="${esc(entry ? entry.entryDate : todayIso())}"></div>
        <label for="je-title">${t('Title')}</label>
        <input class="field" id="je-title" maxlength="200" value="${esc(entry ? entry.title : '')}" placeholder="${esc(t('e.g. Why the Von Braun still feels alive'))}">
      </div>
      <div id="je-editor" style="margin-top:10px"></div>
      <div class="hint" style="margin-top:6px">${t('Some starting points: What stayed with you? What do you notice now that you missed before? What did you find out about how the game was made? · Ctrl+S saves.')}</div>
      <div class="je-shots"><div class="je-shots-head"><b>${t('Screenshots')}</b>
          <span class="hint">${t('A moment from the game that belongs to this entry.')}</span><span class="spacer"></span>
          <button class="btn small" id="je-add-shot" type="button">＋ ${t('Add screenshot…')}</button></div>
        <div id="je-shot-list"></div></div>`,
    footer: `<button class="btn" id="je-cancel">${t('Cancel')}</button><button class="btn gold" id="je-save">${t('Save entry')}</button>`,
    closable: true,
    onKey: (e) => {
      if (e.key === 's' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); return true; }
      if (e.key === 'Escape') { e.preventDefault(); cancel(); return true; }
      return false;
    },
  });
  const q = (id) => m.el.querySelector(`#${id}`);
  const editor = createEditor(q('je-editor'), entry ? entry.content : '');

  // screenshots of the entry: attached on save; new pictures go to the game's gallery first
  let attached = entry ? entry.screenshots.map((x) => ({ id: x.id, thumbnailPath: x.thumbnailPath })) : [];
  const originally = new Set(attached.map((x) => x.id));
  let pending = 0; // pictures added in this editor
  function renderShots() {
    q('je-shot-list').innerHTML = attached.length
      ? `<div class="shot-strip">${attached.map((x) => `<span class="shot attached"><img src="${esc(imgSrc(x.thumbnailPath))}" alt="">
          <button class="shot-x" data-detach="${x.id}" title="${esc(t("Remove from this entry (stays in the game's screenshots)"))}">✕</button></span>`).join('')}</div>`
      : `<p class="hint" style="margin:4px 0 0">${t('No screenshots yet.')}</p>`;
  }
  renderShots();
  const addToEntry = (files) => guard(async () => {
    const gid = Number(q('je-game').value);
    if (!gid) { toast(t('Choose the game first.'), 'err'); return; }
    const added = await addShots(gid, null, files);
    if (!added.length) return;
    pending += added.length;
    attached = [...attached, ...added.map((x) => ({ id: x.id, thumbnailPath: x.thumbnailPath }))];
    q('je-game').disabled = true; // the screenshots belong to this game now
    q('je-game').title = t('Screenshots were added for this game.');
    renderShots();
  });
  q('je-add-shot').addEventListener('click', () => addToEntry(null));
  onFiles(m.el.querySelector('.je-shots'), t('Drop pictures here to add them to this entry'), addToEntry);
  q('je-shot-list').addEventListener('click', (e) => {
    const b = e.target.closest('[data-detach]');
    if (!b) return;
    attached = attached.filter((x) => x.id !== Number(b.dataset.detach));
    renderShots();
  });
  async function saveShots(entryId) {
    const now = new Set(attached.map((x) => x.id));
    const add = [...now].filter((id) => !originally.has(id));
    const remove = [...originally].filter((id) => !now.has(id));
    if (add.length) await api.attachScreenshots(add, entryId);
    if (remove.length) await api.attachScreenshots(remove, null);
  }

  const snapshot = () => JSON.stringify([q('je-game').value, q('je-date').value, q('je-title').value, editor.getHtml(), attached.map((x) => x.id)]);
  const initial = snapshot();
  setTimeout(() => (entry ? editor.focus() : (selected ? q('je-title') : q('je-game')).focus()), 0);

  async function cancel() {
    if (snapshot() !== initial) {
      const discard = await confirmBox({ title: t('Discard changes?'), message: t('The entry has unsaved changes. Discard them?'), ok: t('Discard'), danger: true });
      if (!discard) return;
    }
    if (pending) toast(t("The added screenshots stay in the game's screenshots."));
    m.close(null);
  }
  m.el.querySelector('[data-close]').replaceWith(m.el.querySelector('[data-close]').cloneNode(true));
  m.el.querySelector('[data-close]').addEventListener('click', cancel);
  q('je-cancel').addEventListener('click', cancel);

  let saving = false;
  async function save() {
    if (saving) return;
    const err = q('je-error');
    const input = {
      gameId: Number(q('je-game').value) || 0,
      entryDate: q('je-date').value,
      title: q('je-title').value.trim(),
      content: editor.getHtml(),
    };
    if (!input.gameId) { err.textContent = t('Please choose the game this entry belongs to.'); err.hidden = false; return; }
    saving = true;
    q('je-save').disabled = true;
    try {
      const saved = entry ? await api.updateJournal(entry.id, input) : await api.createJournal(input);
      await saveShots(saved.id);
      toast(entry ? t('Journal entry saved.') : t('Journal entry created.'));
      m.close(saved);
    } catch (e) {
      err.textContent = String(e);
      err.hidden = false;
    } finally {
      saving = false;
      q('je-save').disabled = false;
    }
  }
  q('je-save').addEventListener('click', save);
  return (await m.result) || null;
}

// ---------------------------------------------------------------- page: all games together

let entries = [];
let covers = new Map();
let installed = new Set();

/** Groups entries (newest first) by game; the game with the latest entry comes first. */
function groupByGame(list) {
  const groups = new Map();
  for (const e of list) {
    if (!groups.has(e.gameId)) groups.set(e.gameId, { gameId: e.gameId, title: e.gameTitle, entries: [] });
    groups.get(e.gameId).entries.push(e);
  }
  return [...groups.values()];
}

function groupHtml(g) {
  const thumb = covers.get(g.gameId);
  const latest = g.entries[0].entryDate;
  const first = g.entries[g.entries.length - 1].entryDate;
  const span = first === latest ? formatDate(latest) : `${formatDate(first)} – ${formatDate(latest)}`;
  return `<section class="jgroup" data-group="${g.gameId}">
    <header class="jgroup-head" data-game="${g.gameId}" title="${esc(t('Open {title}', { title: g.title }))}">
      <div class="jgroup-cover">${thumb ? `<img src="${esc(imgSrc(thumb))}" loading="lazy" decoding="async" alt="">` : ''}</div>
      <div><div class="jgroup-title game-title${installed.has(g.gameId) ? ' inst' : ''}">${esc(g.title)}</div>
        <div class="hint">${tn(g.entries.length, '{n} entry', '{n} entries')} · ${span}</div></div>
    </header>
    <div class="entries">${g.entries.map((e) => entryHtml(e, { showGame: false })).join('')}</div>
  </section>`;
}

async function load(focusGame) {
  const list = document.getElementById('jr-list');
  try {
    const [all, games] = await Promise.all([api.listJournal({ order: 'desc' }), api.listGames({})]);
    entries = all;
    covers = new Map(games.map((g) => [g.id, g.thumbnailPath]));
    installed = new Set(games.filter((g) => g.isInstalled).map((g) => g.id));
  } catch (e) {
    list.innerHTML = `<div class="notice err">${esc(e)}</div>`;
    return;
  }
  const groups = groupByGame(entries);
  document.getElementById('jr-count').textContent = entries.length
    ? `${tn(entries.length, '{n} entry', '{n} entries')} · ${tn(groups.length, '{n} game', '{n} games')}`
    : '';
  list.innerHTML = entries.length
    ? `<div class="journal-page">${groups.map(groupHtml).join('')}</div>`
    : `<div class="empty bevel-out"><h2>${t('The journal is empty')}</h2>
       <p>${t('Journal entries are written in the profile of each game – impressions, analysis, things you found out. Here they all come together, game by game.')}</p></div>`;
  if (focusGame) {
    const el = list.querySelector(`[data-group="${focusGame}"]`);
    if (el) el.scrollIntoView({ block: 'start' });
  }
}

export const journalView = {
  async show(params = {}) {
    const main = document.getElementById('main');
    main.innerHTML = `<div class="view">
      <div class="strip"><span>${t('Gaming Journal')}</span><span class="count" id="jr-count"></span></div>
      <div class="library-scroll" id="jr-list"></div></div>`;
    document.getElementById('jr-list').addEventListener('click', (e) => {
      const shot = e.target.closest('[data-shot], [data-shot-all]');
      if (shot) {
        const gid = Number(shot.dataset.shotGame || shot.dataset.shotAll);
        const title = entries.find((x) => x.gameId === gid)?.gameTitle || '';
        return guard(async () => {
          let changed = false;
          const startId = shot.dataset.shot ? Number(shot.dataset.shot) : null;
          await openGallery({ gameId: gid, title, startId, onChange: () => { changed = true; } });
          if (changed) await load(gid);
        });
      }
      const ed = e.target.closest('[data-edit-entry]');
      if (ed) {
        const entry = entries.find((x) => x.id === Number(ed.dataset.editEntry));
        return guard(async () => { if (await openEntryEditor({ entry })) await load(entry.gameId); });
      }
      const del = e.target.closest('[data-delete-entry]');
      if (del) {
        const entry = entries.find((x) => x.id === Number(del.dataset.deleteEntry));
        return guard(async () => { if (await deleteEntry(entry)) await load(); });
      }
      const g = e.target.closest('[data-game]');
      if (g) return navigate('profile', { id: Number(g.dataset.game) });
      return undefined;
    });
    await load(params.gameId);
  },
};
