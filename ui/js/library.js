// Library: grid of cards or compact catalog list, title search, sorting and
// a filter by genre and tags (the funnel button next to Sort).

import { api, imgSrc } from './api.js';
import { esc, initials, debounce, guard, formatBytes, installBox } from './ui.js';
import { state, navigate, CARD_WIDTH } from './store.js';
import { refreshFacets } from './facets.js';
import { openGameForm } from './gameform.js';
import { t, tn } from './i18n.js';

let games = [];
let outsideBound = false;

const FUNNEL = '<svg class="funnel" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M1.5 2.5h13l-5.2 6.2v4.6l-2.6 1.4v-6z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>';
const same = (a, b) => a.toLowerCase() === b.toLowerCase();

function filterCount() {
  return state.query.genres.length + state.query.tags.length;
}

/** Forgets filter values no game has any more (a genre renamed, a tag removed). */
function pruneFilter() {
  const f = state.facets;
  if (!f) return;
  const q = state.query;
  q.genres = q.genres.filter((g) => f.genres.some((x) => same(x.name, g)));
  q.tags = q.tags.filter((tg) => f.tags.some((x) => same(x.name, tg)));
}

function filterListHtml(kind, items, chosen) {
  if (!items.length) {
    return `<div class="hint">${kind === 'genre' ? t('No genres yet \u2013 fill them in when you edit a game.') : t('No tags yet \u2013 add them when you edit a game.')}</div>`;
  }
  return items.map((x) => `<label class="check"><input type="checkbox" data-f${kind}="${esc(x.name)}" ${chosen.some((c) => same(c, x.name)) ? 'checked' : ''}>
    <span class="f-name">${esc(x.name)}</span><span class="f-count muted">${x.count}</span></label>`).join('');
}

function renderFilter() {
  const q = state.query;
  const n = filterCount();
  const btn = document.getElementById('lib-filter');
  if (!btn) return;
  btn.classList.toggle('pressed', n > 0);
  document.getElementById('lib-filter-n').textContent = n ? ` (${n})` : '';
  const pop = document.getElementById('lib-filter-pop');
  if (!pop.hidden) {
    const f = state.facets || { genres: [], tags: [] };
    pop.innerHTML = `<div class="window-title"><span>${t('Filter')}</span><span class="spacer"></span><button class="btn x" id="lib-filter-x" aria-label="${t('Close')}">\u00d7</button></div>
      <div class="filter-cols">
        <div class="filter-col"><div class="filter-title">${t('Genre')}</div><div class="filter-list" id="lib-filter-genres">${filterListHtml('genre', f.genres, q.genres)}</div></div>
        <div class="filter-col"><div class="filter-title">${t('Tags')}</div><div class="filter-list" id="lib-filter-tags">${filterListHtml('tag', f.tags, q.tags)}</div></div>
      </div>
      <div class="filter-foot"><span class="hint">${q.genres.length && q.tags.length ? t('Shows games with any of the ticked genres <b>and</b> any of the ticked tags.') : t('Shows games with any of the ticked genres and/or any of the ticked tags.')}</span>
        <button class="btn small" id="lib-filter-clear" ${n ? '' : 'disabled'}>${t('Clear filters')}</button></div>`;
  }
  const bar = document.getElementById('lib-filter-bar');
  bar.hidden = n === 0;
  const rm = esc(t('Remove this filter'));
  bar.innerHTML = n ? `<span class="muted">${t('Filtered by')}</span>
    ${q.genres.map((g) => `<span class="chip">${esc(g)}<button class="x" data-unfilter="genre" data-name="${esc(g)}" title="${rm}">\u00d7</button></span>`).join('')}
    ${q.genres.length && q.tags.length ? `<span class="muted">${t('and')}</span>` : ''}
    ${q.tags.map((tg) => `<span class="chip tag">${esc(tg)}<button class="x" data-unfilter="tag" data-name="${esc(tg)}" title="${rm}">\u00d7</button></span>`).join('')}
    <button class="btn small" data-act="clearfilter">${t('Clear filters')}</button>` : '';
}

function openFilter(open) {
  const pop = document.getElementById('lib-filter-pop');
  if (!pop) return;
  pop.hidden = !open;
  document.getElementById('lib-filter').setAttribute('aria-expanded', String(open));
  renderFilter();
}

function clearFilter() {
  state.query.genres = [];
  state.query.tags = [];
  renderFilter();
  load();
}

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
  document.getElementById('lib-count').textContent = state.query.search || filterCount() ? t('{shown} of {total}', { shown: games.length, total }) : `${total}`;

  if (total === 0) {
    scroll.innerHTML = `<div class="empty bevel-out">
      <h2>${t('Your archive is empty')}</h2>
      <p>${t('Add a game by choosing its folder, or let the Archive Scanner find the folders with installers for you.')}</p>
      <div class="row"><button class="btn gold" data-act="add">＋ ${t('Add Game')}</button><button class="btn" data-act="scan">${t('Open Archive Scanner')}</button></div></div>`;
    return;
  }
  if (games.length === 0 && filterCount()) {
    const both = state.query.genres.length && state.query.tags.length;
    const why = state.query.search
      ? (both ? t('No game has one of the chosen genres together with one of the chosen tags and that title.') : t('No game has the chosen genres or tags and that title.'))
      : (both ? t('No game has one of the chosen genres together with one of the chosen tags.') : t('No game has the chosen genres or tags.'));
    scroll.innerHTML = `<div class="empty bevel-out"><h2>${t('No game matches the filter')}</h2><p>${why}</p>
      <div class="row"><button class="btn" data-act="clearfilter">${t('Clear filters')}</button>${state.query.search ? `<button class="btn" data-act="clear">${t('Clear search')}</button>` : ''}</div></div>`;
    return;
  }
  if (games.length === 0) {
    scroll.innerHTML = `<div class="empty bevel-out"><h2>${t('No game with that title')}</h2><p>${t('Try a different spelling – diacritics and capitals do not matter.')}</p>
      <div class="row"><button class="btn" data-act="clear">${t('Clear search')}</button></div></div>`;
    return;
  }
  if (state.config.viewMode === 'catalog') {
    scroll.innerHTML = `<table class="list"><thead><tr><th class="inst-col" title="${t('Installed on this PC')}">${t('Inst.')}</th><th class="ico-col" aria-label="${t('Icon')}"></th><th>${t('Title')}</th><th class="num">${t('Year')}</th><th>${t('Genre')}</th><th class="num">${t('Files')}</th><th class="num">${t('Size')}</th><th>${t('Folder')}</th></tr></thead>
      <tbody>${games.map(rowHtml).join('')}</tbody></table>`;
  } else {
    scroll.innerHTML = `<div class="grid">${games.map(cardHtml).join('')}</div>`;
  }
}

async function load() {
  const q = state.query;
  try {
    pruneFilter();
    renderFilter();
    games = await api.listGames({ search: q.search || null, sort: q.sort, genres: q.genres, tags: q.tags });
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
      <div class="strip"><span>${t('Library')}</span><span class="count" id="lib-count"></span></div>
      <div class="toolbar">
        <input class="field search" id="lib-search" type="search" placeholder="${esc(t('Search by title…  (Ctrl+F)'))}" value="${esc(q.search)}">
        <button class="btn gold" id="lib-add">＋ ${t('Add Game')}</button>
        <span class="sep"></span>
        <div class="btn-group">
          <button class="btn ${state.config.viewMode === 'grid' ? 'pressed' : ''}" data-mode="grid" title="${t('Grid View')}">▦ ${t('Grid')}</button>
          <button class="btn ${state.config.viewMode === 'catalog' ? 'pressed' : ''}" data-mode="catalog" title="${t('Catalog View')}">☰ ${t('Catalog')}</button>
        </div>
        <span class="sep"></span>
        <label class="muted" for="lib-sort">${t('Sort')}</label>
        <select class="field" id="lib-sort">
          <option value="title">${t('Title')}</option><option value="year">${t('Release year')}</option>
          <option value="size">${t('Size on disk')}</option><option value="added">${t('Recently added')}</option>
        </select>
        <span class="filter-wrap" id="lib-filter-wrap">
          <button class="btn" id="lib-filter" title="${t('Filter by genre and tags')}" aria-haspopup="true" aria-expanded="false">${FUNNEL}<span>${t('Filter')}</span><span id="lib-filter-n"></span></button>
          <div class="filter-pop window" id="lib-filter-pop" hidden></div>
        </span>
      </div>
      <div class="filter-bar" id="lib-filter-bar" hidden></div>
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
    document.getElementById('lib-filter').addEventListener('click', () => openFilter(document.getElementById('lib-filter-pop').hidden));
    const wrap = document.getElementById('lib-filter-wrap');
    wrap.addEventListener('change', (e) => {
      const box = e.target;
      const kind = box.dataset.fgenre !== undefined ? 'genres' : box.dataset.ftag !== undefined ? 'tags' : null;
      if (!kind) return;
      const name = kind === 'genres' ? box.dataset.fgenre : box.dataset.ftag;
      const list = state.query[kind].filter((x) => !same(x, name));
      if (box.checked) list.push(name);
      state.query[kind] = list;
      load();
    });
    wrap.addEventListener('click', (e) => {
      if (e.target.closest('#lib-filter-x')) openFilter(false);
      else if (e.target.closest('#lib-filter-clear')) clearFilter();
    });
    // close the filter when clicking elsewhere or pressing Esc
    if (!outsideBound) {
      outsideBound = true;
      document.addEventListener('mousedown', (e) => {
        const pop = document.getElementById('lib-filter-pop');
        if (!pop || pop.hidden) return;
        const el = e.target instanceof Element ? e.target : null;
        if (!el || !el.closest('#lib-filter-wrap')) openFilter(false);
      }, true);
    }
    wrap.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.stopPropagation(); openFilter(false); document.getElementById('lib-filter').focus(); }
    });
    document.getElementById('lib-filter-bar').addEventListener('click', (e) => {
      const x = e.target.closest('[data-unfilter]');
      if (x) {
        const kind = x.dataset.unfilter === 'genre' ? 'genres' : 'tags';
        state.query[kind] = state.query[kind].filter((v) => !same(v, x.dataset.name));
        load();
      } else if (e.target.closest('[data-act="clearfilter"]')) clearFilter();
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
        if (act.dataset.act === 'clearfilter') clearFilter();
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
