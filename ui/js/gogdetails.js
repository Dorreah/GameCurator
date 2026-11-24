// "Game details from the internet" on the Archive Scanner page. The user
// picks a source:
//   GOG.com          official title, description, genre, tags, developer,
//                    publisher, year and - for games without one - the cover;
//   Databáze-her.cz  a Czech description, genre, developer and year;
//   MobyGames        an English description, genres, developer, publisher
//                    and year (with the user's own API key, Settings).
// Nothing is saved before the review; filled fields are only replaced when
// ticked (descriptions can be replaced in bulk). These lookups are the only
// things in Game Curator that go online, and only when the button is pressed.
// (The element ids keep their historical gog- prefix.)

import { api, listen } from './api.js';
import { esc, guard, toast, alertBox } from './ui.js';
import { state, navigate } from './store.js';
import { refreshFacets } from './facets.js';
import { t, tn, tm, mark } from './i18n.js';

/** English keys, translated where shown. */
const LABELS = {
  title: mark('Title'), description: mark('Description'), genre: mark('Genre'), tags: mark('Tags'),
  developer: mark('Developer'), publisher: mark('Publisher'), year: mark('Year'), cover: mark('Cover'),
};
const STATUS = {
  found: ['found', mark('Found')],
  check: ['check', mark('Check the match')],
  not_found: ['missing', mark('Not found')],
  error: ['missing', mark('Error')],
};

/** Kinds of products other than a game. */
const PRODUCT_TYPES = { pack: mark('pack'), dlc: mark('dlc') };

/** The sources: name, what they offer, what is sent. */
const SOURCES = {
  gog: {
    site: 'GOG.com',
    offers: mark('Official title, description, genre, tags, developer, publisher, year and &ndash; for games without one &ndash; the cover. In English.'),
    sent: mark('Only the game titles are sent to GOG.com.'),
  },
  dbher: {
    site: 'Databáze-her.cz',
    offers: mark('<b>Czech description</b>, genre, developer and year from the Czech game database.'),
    sent: mark('Game Curator reads the public game pages of Databáze-her.cz, whose addresses it makes from the titles (the site\'s search is not used). The descriptions are licensed CC BY-NC-ND 3.0 CZ: the source is written under each one.'),
  },
  moby: {
    site: 'MobyGames',
    offers: mark('<b>English description</b>, genres, developer, publisher and year from the big game history database.'),
    sent: mark('Through the MobyGames API with your own key: the titles and the key are sent to api.mobygames.com. At most 360 requests an hour (about 180 games).'),
  },
};
const site = (s) => (SOURCES[s] || SOURCES.gog).site;

// kept at module level, so the review survives switching views
let running = false;
let progress = null;
let counts = null;
let scope = 'missing';
let source = null;         // the chosen source (config.detailsSource)
let resultsSource = 'gog'; // the source of the results shown
let replaceDesc = false;   // tick descriptions that are already filled in too
let items = [];            // LookupView from Rust
const picks = new Map();   // gameId -> { include, fields: Set }
let lastError = null;
let finished = null;       // { lookedUp, cancelled } of the last lookup
let imported = [];         // ids of games just imported by the scanner

function root() {
  return state.view === 'scanner' ? document.getElementById('gog-box') : null;
}

function chosenSource() {
  if (!source) source = SOURCES[state.config?.detailsSource] ? state.config.detailsSource : 'gog';
  return source;
}

function short(s, n) {
  const v = String(s).replace(/\s+/g, ' ').trim();
  return v.length > n ? `${v.slice(0, n - 1)}…` : v;
}

/** The fields ticked at first: empty ones, and descriptions when asked to. */
function initialFields(view) {
  const f = new Set(view.changes.filter((c) => c.selected).map((c) => c.field));
  if (replaceDesc && view.changes.some((c) => c.field === 'description')) f.add('description');
  return f;
}

function setItems(list) {
  items = list;
  for (const it of items) {
    if (!picks.has(it.gameId)) picks.set(it.gameId, { include: it.status === 'found', fields: initialFields(it) });
  }
  for (const id of [...picks.keys()]) if (!items.some((i) => i.gameId === id)) picks.delete(id);
}

function replaceItem(view, include) {
  const i = items.findIndex((x) => x.gameId === view.gameId);
  if (i >= 0) items[i] = view; else items.push(view);
  picks.set(view.gameId, { include, fields: initialFields(view) });
}

async function refreshCounts() {
  counts = await api.detailsCounts().catch(() => null);
}

export function initGogEvents() {
  listen('details-progress', (p) => {
    progress = p;
    if (root()) renderProgress();
  });
  listen('details-finished', async (p) => {
    running = false;
    progress = null;
    resultsSource = p.source || resultsSource;
    lastError = p.error ? tm(p.error) : null;
    const list = await api.detailsResults().catch(() => []);
    setItems(list);
    await refreshCounts();
    finished = { lookedUp: p.lookedUp, cancelled: p.cancelled };
    if (root()) render();
    if (p.error) toast(tm(p.error), 'err');
    else toast(tn(items.length, 'Lookup on {site} finished – {n} result to review.', 'Lookup on {site} finished – {n} results to review.', { site: site(resultsSource) }), 'ok');
  });
}

/** The scanner imported games: offer to look them up. */
export function offerForImported(ids) {
  imported = ids;
  if (root()) render();
}

// ---------------------------------------------------------------- rendering

/** "3 games looked up: 2 found, 1 to check." (built when shown, in the current language) */
function finishedNote() {
  if (!finished) return '';
  const n = (st) => items.filter((i) => i.status === st).length;
  const parts = [
    t('{n} found', { n: n('found') }),
    n('check') && t('{n} to check', { n: n('check') }),
    n('not_found') && t('{n} not found', { n: n('not_found') }),
    n('error') && t('{n} with an error', { n: n('error') }),
  ].filter(Boolean).join(', ');
  const p = { parts, site: site(resultsSource) };
  return finished.cancelled
    ? tn(finished.lookedUp, '{site}: {n} game looked up (cancelled): {parts}.', '{site}: {n} games looked up (cancelled): {parts}.', p)
    : tn(finished.lookedUp, '{site}: {n} game looked up: {parts}.', '{site}: {n} games looked up: {parts}.', p);
}

function renderProgress() {
  const el = document.getElementById('gog-progress');
  if (!el) return;
  if (!running) { el.innerHTML = ''; return; }
  const done = progress ? progress.done : 0;
  const total = progress ? progress.total : 0;
  const lit = total ? Math.round((done / total) * 25) : 0;
  el.innerHTML = `<div class="progress">${Array.from({ length: 25 }, (_, i) => `<i class="${i < lit ? 'on' : ''}"></i>`).join('')}</div>
    <div class="scan-status"><span>${t('Looked up:')} <b>${done}</b> / ${total}</span>
    <span class="scan-current">${esc(progress ? progress.title : t('Connecting to {site}…', { site: site(chosenSource()) }))}</span></div>`;
}

function changeRow(it, c) {
  const p = picks.get(it.gameId);
  const on = p.fields.has(c.field);
  let what;
  if (c.field === 'cover') {
    what = `<span class="gog-new">${t('New cover from GOG.com')}</span>`;
  } else if (c.field === 'tags') {
    what = `${c.current ? `<span class="gog-old">${esc(c.current)}</span> ` : ''}<span class="gog-new">+ ${esc(c.proposed)}</span>`;
  } else if (c.field === 'description') {
    what = `${c.current ? `<span class="gog-old">${esc(short(c.current, 60))}</span> → ` : ''}<span class="gog-new" title="${esc(short(c.proposed, 1200))}">${esc(short(c.proposed, 180))}</span>`;
  } else {
    what = `${c.current ? `<span class="gog-old">${esc(c.current)}</span> → ` : `<span class="gog-old muted">${t('(empty)')}</span> → `}<span class="gog-new">${esc(c.proposed)}</span>`;
  }
  return `<tr class="${c.selected ? '' : 'keep'}"><td><input type="checkbox" data-gog-field="${it.gameId}" value="${c.field}" ${on ? 'checked' : ''}></td>
    <td class="gog-label">${LABELS[c.field] ? t(LABELS[c.field]) : esc(c.field)}</td><td>${what}${!c.selected && !on ? ` <span class="hint">${t('(yours is kept unless ticked)')}</span>` : ''}</td></tr>`;
}

function itemHtml(it) {
  const p = picks.get(it.gameId);
  const s = site(it.source || resultsSource);
  const [cls, labelKey] = STATUS[it.status] || STATUS.error;
  const label = t(labelKey);
  const chosen = it.candidates.find((c) => c.id === it.chosen);
  const others = it.candidates.length > 1 || (!chosen && it.candidates.length)
    ? `<select class="field gog-other" data-gog-choose="${it.gameId}" title="${esc(t('Other products found on {site}', { site: s }))}">
        ${chosen ? '' : `<option value="">${t('Choose a product…')}</option>`}
        ${it.candidates.map((c) => `<option value="${esc(c.id)}" ${c.id === it.chosen ? 'selected' : ''}>${esc(c.title)}${c.year ? ` (${c.year})` : ''}${c.productType !== 'game' ? ` – ${esc(PRODUCT_TYPES[c.productType] ? t(PRODUCT_TYPES[c.productType]) : c.productType)}` : ''}</option>`).join('')}
      </select>` : '';
  const page = chosen && it.storeLink
    ? ` <button class="btn small link" data-gog-page="${it.gameId}" title="${esc(it.storeLink)}">${t('Game page')} ↗</button>` : '';
  const match = chosen
    ? `<span class="gog-match">${esc(s)}: <b>${esc(chosen.title)}</b>${chosen.year ? ` (${chosen.year})` : ''}${chosen.developers.length ? ` · ${esc(chosen.developers.join(', '))}` : ''}${page}</span>`
    : `<span class="gog-match muted">${t('No matching game on {site}.', { site: esc(s) })}</span>`;
  const changes = it.changes.length
    ? `<table class="gog-changes"><tbody>${it.changes.map((c) => changeRow(it, c)).join('')}</tbody></table>`
    : (chosen ? `<div class="hint">${t('Everything {site} knows is already in your catalog.', { site: esc(s) })}</div>` : '');
  const search = !chosen || it.status === 'check'
    ? `<div class="row gog-search"><input class="field" data-gog-q="${it.gameId}" value="${esc(it.title)}" maxlength="300"
          placeholder="${esc(t('Title, or the address of the game page on {site}', { site: s }))}" aria-label="${esc(t('Title to look for'))}">
        <button class="btn small" data-gog-search="${it.gameId}">${t('Search again')}</button>
        <button class="btn small" data-gog-site="${it.gameId}" title="${esc(t('Opens the search of {site} in your browser. Copy the address of the right game page and paste it here.', { site: s }))}">${t('Search on {site}', { site: esc(s) })} ↗</button></div>` : '';
  return `<div class="gog-item${p.include && chosen ? ' on' : ''}" data-gid="${it.gameId}">
    <div class="gog-head">
      <label class="check"><input type="checkbox" data-gog-inc="${it.gameId}" ${p.include && chosen ? 'checked' : ''} ${chosen && it.changes.length ? '' : 'disabled'}>
        <b class="gog-title">${esc(it.title)}</b></label>
      <span class="badge ${cls}">${label}</span>${match}${others}
    </div>
    ${it.error ? `<div class="notice err">${esc(tm(it.error))}</div>` : ''}
    <div class="gog-body">${it.coverPreview ? `<img class="gog-cover" src="${esc(it.coverPreview)}" alt="${t('Cover from GOG.com')}">` : ''}${changes}</div>
    ${search}
  </div>`;
}

function toSave() {
  return items.filter((it) => {
    const p = picks.get(it.gameId);
    return p && p.include && it.chosen && it.changes.some((c) => p.fields.has(c.field));
  });
}

function renderResults() {
  const el = document.getElementById('gog-results');
  if (!el) return;
  if (!items.length) {
    el.innerHTML = finished ? `<div class="hint">${esc(finishedNote())}</div>` : '';
    return;
  }
  const n = toSave().length;
  el.innerHTML = `<div class="row" style="margin:10px 0 6px">${esc(finishedNote())}</div>
    <div class="gog-list">${items.map(itemHtml).join('')}</div>
    <div class="row sticky-actions">
      <button class="btn gold" id="gog-save" ${n ? '' : 'disabled'}>${tn(n, 'Save details for {n} game', 'Save details for {n} games')}</button>
      <button class="btn" id="gog-close">${t('Close results')}</button>
      <span class="hint">${t('Only ticked games and details are saved. Game files are never touched.')}</span></div>`;
}

function sourceHtml() {
  const cur = chosenSource();
  return Object.entries(SOURCES).map(([key, s]) => {
    const keyNote = key === 'moby'
      ? (counts && counts.mobyKey
        ? `<span class="src-key ok">${t('API key: set')}</span>`
        : `<span class="src-key">${t('API key: not set')} – <button class="btn small link" data-act="moby-key">${t('add it in Settings')}</button></span>`)
      : '';
    return `<label class="src-option${cur === key ? ' on' : ''}">
      <input type="radio" name="gog-source" value="${key}" ${cur === key ? 'checked' : ''} ${running ? 'disabled' : ''}>
      <span><b class="src-name">${esc(s.site)}</b> ${keyNote}<br><span class="hint">${t(s.offers)}</span></span></label>`;
  }).join('');
}

function render() {
  const box = root();
  if (!box) return;
  const cur = chosenSource();
  document.getElementById('gog-source').innerHTML = sourceHtml();
  document.getElementById('gog-sent').innerHTML = t(SOURCES[cur].sent);
  const start = document.getElementById('gog-start');
  start.disabled = running;
  start.textContent = t('Fetch details from {site}', { site: site(cur) });
  document.getElementById('gog-cancel').disabled = !running;
  const sel = document.getElementById('gog-scope');
  sel.disabled = running;
  if (counts) {
    sel.innerHTML = `<option value="missing" ${scope === 'missing' ? 'selected' : ''}>${t('Games with missing details ({n})', { n: counts.missing })}</option>
      <option value="all" ${scope === 'all' ? 'selected' : ''}>${t('All games ({n})', { n: counts.all })}</option>`;
  }
  document.getElementById('gog-replace').checked = replaceDesc;
  const offer = document.getElementById('gog-imported');
  offer.innerHTML = imported.length && !running
    ? `<div class="notice info row">${tn(imported.length, '{n} game was just imported.', '{n} games were just imported.')}
        <button class="btn small gold" id="gog-start-imported">${t('Fetch their details from {site}', { site: esc(site(cur)) })}</button></div>` : '';
  if (lastError) document.getElementById('gog-error').innerHTML = `<div class="notice err">${esc(lastError)}</div>`;
  else document.getElementById('gog-error').innerHTML = '';
  renderProgress();
  renderResults();
}

// ---------------------------------------------------------------- actions

async function start(sc, ids) {
  const src = chosenSource();
  running = true;
  lastError = null;
  finished = null;
  progress = null;
  items = [];
  picks.clear();
  resultsSource = src;
  if (sc === 'ids') imported = [];
  render();
  const ok = await guard(async () => { await api.detailsStart(src, sc, ids || null); return true; });
  if (!ok) { running = false; render(); }
}

async function save() {
  const list = toSave().map((it) => ({
    gameId: it.gameId,
    productId: it.chosen,
    fields: it.changes.map((c) => c.field).filter((f) => picks.get(it.gameId).fields.has(f)),
  }));
  if (!list.length) return;
  const btn = document.getElementById('gog-save');
  btn.disabled = true;
  btn.textContent = t('Saving…');
  const report = await guard(() => api.detailsApply(list));
  if (!report) { renderResults(); return; }
  items = items.filter((it) => !report.savedIds.includes(it.gameId));
  for (const id of report.savedIds) picks.delete(id);
  await refreshCounts();
  await refreshFacets();
  render();
  toast(tn(report.saved, 'Details from {site} saved for {n} game.', 'Details from {site} saved for {n} games.', { site: site(resultsSource) }), 'ok');
  if (report.errors.length) await alertBox(`${t('Some details could not be saved:')}\n\n${report.errors.map((x) => tm(x)).join('\n')}`, { kind: 'warn' });
}

async function chooseSource(key) {
  if (!SOURCES[key] || key === source) return;
  source = key;
  imported = imported.length ? imported : [];
  if (state.config && state.config.detailsSource !== key) {
    try {
      state.config = await api.saveConfig({ ...state.config, detailsSource: key });
    } catch (_) { /* kept for this session */ }
  }
  render();
}

/** Ticks (or unticks) the offered descriptions of all results at once. */
function setReplaceDesc(on) {
  replaceDesc = on;
  for (const it of items) {
    const c = it.changes.find((x) => x.field === 'description');
    if (!c || c.selected) continue;
    const p = picks.get(it.gameId);
    if (on) { p.fields.add('description'); if (it.chosen && it.status !== 'check') p.include = true; } else p.fields.delete('description');
  }
  renderResults();
}

function bind(box) {
  box.addEventListener('change', (e) => {
    const el = e.target;
    if (el.id === 'gog-scope') { scope = el.value; return; }
    if (el.name === 'gog-source') { chooseSource(el.value); return; }
    if (el.id === 'gog-replace') { setReplaceDesc(el.checked); return; }
    if (el.dataset.gogInc) {
      picks.get(Number(el.dataset.gogInc)).include = el.checked;
      renderResults();
    } else if (el.dataset.gogField) {
      const p = picks.get(Number(el.dataset.gogField));
      if (el.checked) { p.fields.add(el.value); p.include = true; } else p.fields.delete(el.value);
      renderResults();
    } else if (el.dataset.gogChoose && el.value) {
      const gid = Number(el.dataset.gogChoose);
      guard(async () => {
        el.disabled = true;
        replaceItem(await api.detailsChoose(gid, el.value), true);
        renderResults();
      }).finally(() => { el.disabled = false; });
    }
  });
  box.addEventListener('click', (e) => {
    const el = e.target;
    if (el.closest('#gog-start')) start(scope);
    else if (el.closest('#gog-start-imported')) start('ids', imported);
    else if (el.closest('#gog-cancel')) guard(() => api.detailsCancel());
    else if (el.closest('#gog-save')) save();
    else if (el.closest('[data-act="moby-key"]')) { e.preventDefault(); navigate('settings', { focus: 'st-moby-key' }); }
    else if (el.closest('#gog-close')) {
      guard(async () => {
        await api.detailsClear();
        items = [];
        picks.clear();
        finished = null;
        render();
      });
    } else if (el.dataset.gogPage) {
      guard(() => api.detailsOpenPage(Number(el.dataset.gogPage)));
    } else if (el.dataset.gogSite) {
      const gid = Number(el.dataset.gogSite);
      const q = box.querySelector(`[data-gog-q="${gid}"]`).value;
      guard(() => api.detailsOpenSearch(resultsSource, q));
    } else if (el.dataset.gogSearch) {
      const gid = Number(el.dataset.gogSearch);
      const q = box.querySelector(`[data-gog-q="${gid}"]`).value;
      el.disabled = true;
      el.textContent = t('Searching…');
      guard(async () => {
        const v = await api.detailsSearch(gid, q);
        replaceItem(v, v.status === 'found');
        renderResults();
      }).finally(() => { el.disabled = false; el.textContent = t('Search again'); });
    }
  });
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target.dataset.gogQ) {
      e.preventDefault();
      box.querySelector(`[data-gog-search="${e.target.dataset.gogQ}"]`)?.click();
    }
  });
}

/** The section on the Archive Scanner page. */
export function gogSection() {
  return `<fieldset class="group" id="gog-box"><legend>${t('Game details from the internet')}</legend>
    <div class="hint" style="margin-bottom:8px">${t('Looks your games up at the source you choose and offers what it knows. You review everything before it is saved; details you filled in yourself stay unless you tick them. These lookups are the only things in Game Curator that go online, and only when you press the button.')}</div>
    <div class="src-options" id="gog-source" role="radiogroup" aria-label="${esc(t('Source'))}"></div>
    <div class="hint src-sent" id="gog-sent"></div>
    <div id="gog-imported"></div>
    <div class="row">
      <select class="field" id="gog-scope" style="flex:none;width:300px"><option value="missing">${t('Games with missing details')}</option></select>
      <button class="btn gold" id="gog-start"></button>
      <button class="btn" id="gog-cancel" disabled>${t('Cancel')}</button>
      <label class="check" style="margin-left:10px" title="${esc(t('Ticks the offered description of every game, so it replaces the one you have (you can still untick single games).'))}">
        <input type="checkbox" id="gog-replace"> ${t('Replace descriptions that are already filled in')}</label></div>
    <div id="gog-progress" style="margin-top:10px"></div>
    <div id="gog-error"></div>
    <div id="gog-results"></div>
  </fieldset>`;
}

export async function showGogSection() {
  const box = root();
  if (!box) return;
  bind(box);
  render();
  await refreshCounts();
  render();
}
