// Library: grid of cards or compact catalog list, title search and sorting.

import { api, imgSrc } from './api.js';
import { esc, initials, debounce, guard, formatBytes, installBox } from './ui.js';
import { state, navigate, CARD_WIDTH } from './store.js';
import { refreshFacets } from './facets.js';
import { openGameForm } from './gameform.js';

let games = [];

function cardHtml(g) {
  const cover = g.thumbnailPath
    ? `<img src="${esc(imgSrc(g.thumbnailPath))}" loading="lazy" decoding="async" alt="">`
    : `<div class="ph">${esc(initials(g.title))}<span></span></div>`;
  const meta = [g.releaseYear, g.fileCount ? formatBytes(g.totalSize) : null].filter(Boolean).join(' · ');
  return `<button class="card${g.isAvailable ? '' : ' offline'}" data-id="${g.id}" title="${esc(g.title)}">
    <div class="cover">${cover}${g.isAvailable ? '' : '<span class="badge offline stamp">OFFLINE</span>'}</div>
    <div class="meta"><div class="t game-title${g.isInstalled ? ' inst' : ''}">${esc(g.title)}</div><div class="s"><span>${esc(meta)}</span>${installBox(g.isInstalled)}</div></div>
  </button>`;
}

function rowHtml(g) {
  return `<tr class="click${g.isAvailable ? '' : ' offline'}" data-id="${g.id}">
    <td class="inst-col">${installBox(g.isInstalled)}</td>
    <td class="ico-col">${g.isInstalled && g.iconPath ? `<img class="game-icon" src="${esc(imgSrc(g.iconPath))}" alt="" loading="lazy" decoding="async">` : ''}</td>
    <td><b class="game-title${g.isInstalled ? ' inst' : ''}">${esc(g.title)}</b>${g.isAvailable ? '' : ' <span class="badge offline">OFFLINE</span>'}</td>
    <td class="num">${g.releaseYear ?? ''}</td><td>${esc(g.genre)}</td>
    <td class="num">${g.fileCount || ''}</td><td class="num">${g.fileCount ? formatBytes(g.totalSize) : ''}</td>
    <td class="mono muted">${esc(g.folderPath)}</td></tr>`;
}

function render() {
  const scroll = document.getElementById('lib-scroll');
  if (!scroll) return;
  const total = state.facets ? state.facets.total : games.length;
  document.getElementById('lib-count').textContent = state.query.search ? `${games.length} of ${total}` : `${total}`;

  if (total === 0) {
    scroll.innerHTML = `<div class="empty bevel-out">
      <h2>Your archive is empty</h2>
      <p>Add a game by choosing its folder, or let the Archive Scanner find the folders with installers for you.</p>
      <div class="row"><button class="btn gold" data-act="add">＋ Add Game</button><button class="btn" data-act="scan">Open Archive Scanner</button></div></div>`;
    return;
  }
  if (games.length === 0) {
    scroll.innerHTML = `<div class="empty bevel-out"><h2>No game with that title</h2><p>Try a different spelling – diacritics and capitals do not matter.</p>
      <div class="row"><button class="btn" data-act="clear">Clear search</button></div></div>`;
    return;
  }
  if (state.config.viewMode === 'catalog') {
    scroll.innerHTML = `<table class="list"><thead><tr><th class="inst-col" title="Installed on this PC">Inst.</th><th class="ico-col" aria-label="Icon"></th><th>Title</th><th class="num">Year</th><th>Genre</th><th class="num">Files</th><th class="num">Size</th><th>Folder</th></tr></thead>
      <tbody>${games.map(rowHtml).join('')}</tbody></table>`;
  } else {
    scroll.innerHTML = `<div class="grid">${games.map(cardHtml).join('')}</div>`;
  }
}

async function load() {
  const q = state.query;
  try {
    games = await api.listGames({ search: q.search || null, sort: q.sort });
  } catch (e) {
    games = [];
    const scroll = document.getElementById('lib-scroll');
    if (scroll) scroll.innerHTML = `<div class="notice err">${esc(e)}</div>`;
    return;
  }
  render();
}

function clearSearch() {
  state.query.search = '';
  const s = document.getElementById('lib-search');
  if (s) s.value = '';
  load();
}

async function saveConfig(patch) {
  Object.assign(state.config, patch);
  try { state.config = await api.saveConfig(state.config); } catch (_) { /* non-critical */ }
}

export async function addGame() {
  const id = await openGameForm(null);
  if (id) {
    await refreshFacets();
    if (state.view === 'library') await load();
  }
}

export const libraryView = {
  async show() {
    const main = document.getElementById('main');
    const q = state.query;
    if (!q.sort) q.sort = state.config.sort || 'title';
    main.innerHTML = `<div class="view" id="library">
      <div class="strip"><span>Library</span><span class="count" id="lib-count"></span></div>
      <div class="toolbar">
        <input class="field search" id="lib-search" type="search" placeholder="Search by title…  (Ctrl+F)" value="${esc(q.search)}">
        <button class="btn gold" id="lib-add">＋ Add Game</button>
        <span class="sep"></span>
        <div class="btn-group">
          <button class="btn ${state.config.viewMode === 'grid' ? 'pressed' : ''}" data-mode="grid" title="Grid View">▦ Grid</button>
          <button class="btn ${state.config.viewMode === 'catalog' ? 'pressed' : ''}" data-mode="catalog" title="Catalog View">☰ Catalog</button>
        </div>
        <span class="sep"></span>
        <label class="muted" for="lib-sort">Sort</label>
        <select class="field" id="lib-sort">
          <option value="title">Title</option><option value="year">Release year</option>
          <option value="size">Size on disk</option><option value="added">Recently added</option>
        </select>
      </div>
      <div class="library-scroll" id="lib-scroll" style="--card-w:${CARD_WIDTH[state.config.cardSize] || 168}px"></div>
    </div>`;
    document.getElementById('lib-sort').value = q.sort;

    const search = document.getElementById('lib-search');
    search.addEventListener('input', debounce(() => { q.search = search.value.trim(); load(); }, 140));
    document.getElementById('lib-add').addEventListener('click', addGame);
    document.getElementById('lib-sort').addEventListener('change', (e) => {
      q.sort = e.target.value;
      saveConfig({ sort: q.sort });
      load();
    });
    main.querySelectorAll('[data-mode]').forEach((b) => b.addEventListener('click', () => {
      main.querySelectorAll('[data-mode]').forEach((x) => x.classList.toggle('pressed', x === b));
      saveConfig({ viewMode: b.dataset.mode });
      render();
    }));
    document.getElementById('lib-scroll').addEventListener('click', (e) => {
      const act = e.target.closest('[data-act]');
      if (act) {
        if (act.dataset.act === 'add') addGame();
        if (act.dataset.act === 'scan') navigate('scanner');
        if (act.dataset.act === 'clear') clearSearch();
        return;
      }
      const item = e.target.closest('[data-id]');
      if (item) navigate('profile', { id: Number(item.dataset.id) });
    });
    await load();
  },
  focusSearch() {
    const s = document.getElementById('lib-search');
    if (s) { s.focus(); s.select(); }
  },
  reload: () => guard(load),
};
