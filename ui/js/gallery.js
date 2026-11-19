// Screenshots: the small strip in the game profile and the gallery window
// (all screenshots of a game, one large picture with caption, add, delete).

import { api, imgSrc } from './api.js';
import { esc, formatDate, toast, confirmBox, openModal, guard } from './ui.js';
import { onFiles } from './dropzone.js';

export const SHOT_DROP_HINT = 'Drop pictures here to add them as screenshots';

export const SHOT_PROMPT = 'Discovered or experienced something interesting in the game? Capture it!';

/**
 * Adds pictures chosen in a dialog (or dropped: `files` with tokens); reports
 * the ones that failed. Returns the added screenshots.
 */
export async function addShots(gameId, entryId = null, files = null) {
  const r = files
    ? await api.addDroppedScreenshots(gameId, entryId, files.map((f) => f.token))
    : await api.addScreenshots(gameId, entryId);
  if (r.failed.length) {
    toast(`${r.failed.length} picture(s) could not be added: ${r.failed.join('; ')}`, 'err');
  }
  if (r.added.length) toast(r.added.length === 1 ? 'Screenshot added.' : `${r.added.length} screenshots added.`);
  return r.added;
}

/** A row of small 16:9 previews; `data-shot` opens the gallery at that picture. */
export function shotStrip(shots, { max = 3, gameId, moreTile = true } = {}) {
  const shown = shots.slice(0, max);
  const more = moreTile ? shots.length - shown.length : 0;
  return `<div class="shot-strip">${shown.map((s) => `<button class="shot" data-shot="${s.id}" data-shot-game="${gameId}"
      title="${esc(s.caption || 'Open the screenshot')}"><img src="${esc(imgSrc(s.thumbnailPath))}" alt="" loading="lazy" decoding="async"></button>`).join('')}
    ${more > 0 ? `<button class="shot more" data-shot-all="${gameId}" title="All screenshots">+${more}</button>` : ''}</div>`;
}

/** Content of the Screenshots panel in the game profile. */
export function shotsPanelHtml(shots, gameId) {
  if (!shots.length) {
    return `<p class="shot-prompt">${esc(SHOT_PROMPT)}</p>
      <button class="btn" id="pf-add-shot">＋ Add screenshots…</button> <span class="hint">or drag pictures here</span>`;
  }
  return `${shotStrip(shots, { max: 3, gameId, moreTile: false })}
    <div class="shot-actions">
      <button class="btn small" id="pf-all-shots">${shots.length > 3 ? `All ${shots.length} screenshots` : 'Open the gallery'}</button>
      <button class="btn small" id="pf-add-shot">＋ Add…</button>
      <span class="shot-prompt small">${esc(SHOT_PROMPT)}</span>
    </div>`;
}

function meta(s) {
  const parts = [];
  if (s.takenAt) parts.push(`Taken ${formatDate(s.takenAt.slice(0, 10))}`);
  else parts.push(`Added ${formatDate(s.dateAdded.slice(0, 10))}`);
  if (s.width) parts.push(`${s.width}×${s.height}`);
  if (s.originalName) parts.push(esc(s.originalName));
  return parts.join(' · ');
}

/**
 * The gallery window of a game. Opens at `startId` (large view) or as a grid.
 * `onChange` runs after something was added, captioned or deleted.
 */
export async function openGallery({ gameId, title, startId = null, onChange = () => {} }) {
  let shots = await api.listScreenshots(gameId);
  let current = startId ? shots.findIndex((s) => s.id === startId) : -1;
  const m = openModal({
    title: `Screenshots – ${title}`,
    width: Math.min(1120, window.innerWidth - 40),
    body: '<div id="gal"></div>',
    onKey: (e) => {
      if (current < 0 || e.target.closest('input, textarea')) return false;
      if (e.key === 'ArrowLeft') { e.preventDefault(); show(current - 1); return true; }
      if (e.key === 'ArrowRight') { e.preventDefault(); show(current + 1); return true; }
      return false;
    },
  });
  m.el.querySelector('.window').classList.add('gallery-window');
  const root = m.el.querySelector('#gal');
  onFiles(m.el.querySelector('.gallery-window'), SHOT_DROP_HINT, (files) => guard(async () => {
    const added = await addShots(gameId, null, files);
    if (added.length) {
      shots = [...added.reverse(), ...shots];
      grid();
      onChange();
    }
  }));

  function grid() {
    current = -1;
    root.innerHTML = `<div class="gal-head">
        <span class="muted">${shots.length ? `${shots.length} screenshot${shots.length === 1 ? '' : 's'}` : 'No screenshots yet'}</span>
        <span class="spacer"></span>
        <button class="btn" id="gal-add">＋ Add screenshots…</button></div>
      <p class="shot-prompt">${esc(SHOT_PROMPT)}</p>
      <div class="gal-grid">${shots.map((s, i) => `<button class="gal-item" data-i="${i}" title="${esc(s.caption || '')}">
          <span class="gal-thumb"><img src="${esc(imgSrc(s.thumbnailPath))}" alt="" loading="lazy" decoding="async"></span>
          <span class="gal-cap">${s.caption ? esc(s.caption) : `<span class="faint">${esc(formatDate((s.takenAt || s.dateAdded).slice(0, 10)))}</span>`}</span>
          ${s.journalEntryId ? '<span class="gal-badge" title="Belongs to a journal entry">journal</span>' : ''}
        </button>`).join('')}</div>`;
  }

  function show(i) {
    if (!shots.length) return grid();
    current = (i + shots.length) % shots.length;
    const s = shots[current];
    root.innerHTML = `<div class="gal-view">
        <button class="gal-nav prev" id="gal-prev" title="Previous (←)" ${shots.length < 2 ? 'disabled' : ''}>‹</button>
        <div class="gal-pic"><img src="${esc(imgSrc(s.imagePath))}" alt="${esc(s.caption)}"></div>
        <button class="gal-nav next" id="gal-next" title="Next (→)" ${shots.length < 2 ? 'disabled' : ''}>›</button>
      </div>
      <div class="gal-info">
        <input class="field" id="gal-caption" maxlength="500" placeholder="Add a caption – what happened here?" value="${esc(s.caption)}">
        <div class="hint">${current + 1} / ${shots.length} · ${meta(s)}
          ${s.journalEntryId ? ` · from the journal entry <b>${esc(s.entryTitle || '(untitled)')}</b> (${esc(formatDate(s.entryDate))})` : ''}</div>
        <div class="gal-buttons">
          <button class="btn small" id="gal-grid">◄ All screenshots</button>
          <span class="spacer"></span>
          <button class="btn small" id="gal-open" title="Open in the Windows photo viewer">Open in viewer</button>
          <button class="btn small danger-text" id="gal-delete">Delete</button>
        </div>
      </div>`;
  }

  async function saveCaption() {
    const input = root.querySelector('#gal-caption');
    if (!input || current < 0) return;
    const s = shots[current];
    if (input.value.trim() === s.caption) return;
    shots[current] = await api.setScreenshotCaption(s.id, input.value);
    toast('Caption saved.');
    onChange();
  }

  root.addEventListener('click', (e) => guard(async () => {
    const t = e.target;
    const item = t.closest('[data-i]');
    if (item) return show(Number(item.dataset.i));
    if (t.closest('#gal-prev')) { await saveCaption(); return show(current - 1); }
    if (t.closest('#gal-next')) { await saveCaption(); return show(current + 1); }
    if (t.closest('#gal-grid')) { await saveCaption(); return grid(); }
    if (t.closest('#gal-open')) return api.openScreenshot(shots[current].id);
    if (t.closest('#gal-add')) {
      const added = await addShots(gameId);
      if (added.length) {
        shots = [...added.reverse(), ...shots];
        grid();
        onChange();
      }
      return undefined;
    }
    if (t.closest('#gal-delete')) {
      const s = shots[current];
      const ok = await confirmBox({
        title: 'Delete screenshot?',
        html: `Delete this screenshot${s.caption ? ` (<b>${esc(s.caption)}</b>)` : ''}?\n\nThe copy stored in the archive is deleted. Your original picture file is not touched.`,
        ok: 'Delete',
        danger: true,
      });
      if (!ok) return undefined;
      await api.deleteScreenshot(s.id);
      shots.splice(current, 1);
      toast('Screenshot deleted.');
      onChange();
      return shots.length ? show(Math.min(current, shots.length - 1)) : grid();
    }
    return undefined;
  }));
  root.addEventListener('keydown', (e) => {
    if (e.target.id === 'gal-caption' && e.key === 'Enter') { e.preventDefault(); guard(saveCaption); }
  });
  root.addEventListener('focusout', (e) => { if (e.target.id === 'gal-caption') guard(saveCaption); });

  if (current >= 0) show(current); else grid();
  await m.result;
}
