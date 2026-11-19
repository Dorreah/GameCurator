// Add Game / Edit Game form (modal window).

import { api, imgSrc } from './api.js';
import { esc, openModal, toast, initials } from './ui.js';
import { state } from './store.js';
import { createCropper } from './cropper.js';

function titleFromFolder(path) {
  const name = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || '';
  return name
    .replace(/[[(].*?[\])]/g, ' ')
    .replace(/_/g, ' ')
    .replace(/\bv?\d+(\.\d+)+\w*\b/gi, ' ')
    .replace(/\b(gog|setup|installer)\b/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Opens the form. `game` is a GameDetail for editing or null to add.
 * Resolves to the game id after saving, or null when cancelled.
 */
export function openGameForm(game) {
  const editing = !!game;
  const g = game || {};
  const facets = state.facets || { genres: [], tags: [] };
  let artwork = { action: 'keep' };
  // the whole picture (cover) is shown in the crop editor; the card gets the chosen part
  let coverPath = g.artworkPath || null;
  let coverCrop = g.coverCrop || null;
  let cropper = null;

  const body = `
    <div class="notice err" id="gf-error" hidden></div>
    <div style="display:grid;grid-template-columns:1fr 200px;gap:18px">
      <div class="form-grid">
        <label class="req" for="gf-title">Title</label>
        <input class="field" id="gf-title" maxlength="200" value="${esc(g.title || '')}" autofocus>
        <label class="req" for="gf-folder">Game folder</label>
        <div class="row"><input class="field mono" id="gf-folder" value="${esc(g.folderResolved || '')}" placeholder="Folder with the installer files">
          <button class="btn" id="gf-browse" type="button">Browse…</button></div>
        <label for="gf-dev">Developer</label><input class="field" id="gf-dev" maxlength="200" value="${esc(g.developer || '')}">
        <label for="gf-pub">Publisher</label><input class="field" id="gf-pub" maxlength="200" value="${esc(g.publisher || '')}">
        <label for="gf-year">Release year</label>
        <div class="row"><input class="field" id="gf-year" type="number" min="1940" max="2100" style="max-width:110px;flex:none" value="${g.releaseYear ?? ''}">
          <label for="gf-genre" class="muted" style="margin-left:12px">Genre</label>
          <input class="field" id="gf-genre" list="gf-genres" maxlength="100" value="${esc(g.genre || '')}"></div>
        <datalist id="gf-genres">${facets.genres.map((x) => `<option value="${esc(x.name)}">`).join('')}</datalist>
        <label for="gf-tags">Tags</label>
        <input class="field" id="gf-tags" value="${esc((g.tags || []).join(', '))}" placeholder="Comma separated, e.g. GOG, big box, Infinity Engine">
        <div></div><div class="hint">${facets.tags.length ? `Existing tags: ${facets.tags.slice(0, 14).map((t) => esc(t.name)).join(', ')}` : 'Tags help you group games your own way.'}</div>
        <label for="gf-desc" style="align-self:start;margin-top:4px">Description</label>
        <textarea class="field" id="gf-desc" rows="5" maxlength="20000">${esc(g.description || '')}</textarea>
      </div>
      <div>
        <div id="gf-cover"></div>
        <div style="display:flex;flex-direction:column;gap:6px;margin-top:8px">
          <button class="btn" id="gf-art" type="button">Choose image…</button>
          <button class="btn small" id="gf-art-remove" type="button">Remove image</button>
          <div class="hint" id="gf-art-hint">JPG, PNG or WebP. An optimized WebP copy is stored in the archive.</div>
        </div>
      </div>
    </div>
    <p class="hint" style="margin:10px 0 0">Only the title and the folder are required. Game Curator never moves, changes or deletes files in the game folder.</p>`;

  const m = openModal({
    title: editing ? `Edit game – ${g.title}` : 'Add Game',
    width: 840,
    body,
    footer: `<button class="btn" data-close>Cancel</button><button class="btn gold" id="gf-save">${editing ? 'Save changes' : 'Add to catalog'}</button>`,
    onKey: (e) => {
      if (e.key === 's' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); return true; }
      return false;
    },
  });
  const $ = (id) => m.el.querySelector(`#${id}`);

  function renderCover() {
    const box = $('gf-cover');
    const title = $('gf-title').value || '?';
    if (coverPath) {
      cropper = createCropper(box, { src: imgSrc(coverPath), crop: coverCrop, width: 200 });
      $('gf-art-hint').textContent = 'Drag the picture to choose what the card shows; zoom with the wheel or the slider.';
    } else {
      cropper = null;
      box.innerHTML = `<div class="cover bevel-out" style="width:200px"><div class="ph" style="position:absolute;inset:0">${esc(initials(title))}<span></span></div></div>`;
      $('gf-art-hint').textContent = 'JPG, PNG or WebP. An optimized WebP copy is stored in the archive.';
    }
    $('gf-art-remove').disabled = !coverPath;
  }
  renderCover();
  $('gf-title').addEventListener('input', () => { if (!coverPath) renderCover(); });

  $('gf-browse').addEventListener('click', async () => {
    try {
      const start = $('gf-folder').value || (state.config && state.config.scanRoot) || (state.info && state.info.gamesDir);
      const picked = await api.pickFolder('Choose the game folder', start);
      if (picked) {
        $('gf-folder').value = picked;
        if (!$('gf-title').value.trim()) { $('gf-title').value = titleFromFolder(picked); renderCover(); }
      }
    } catch (e) { showError(e); }
  });
  $('gf-art').addEventListener('click', async () => {
    const btn = $('gf-art');
    btn.disabled = true;
    btn.textContent = 'Processing…';
    try {
      const art = await api.pickArtwork();
      if (art) {
        artwork = { action: 'set', artworkPath: art.artworkPath, thumbnailPath: art.thumbnailPath };
        coverPath = art.artworkPath;
        coverCrop = null;
        renderCover();
      }
    } catch (e) { showError(e); } finally {
      btn.disabled = false;
      btn.textContent = 'Choose image…';
    }
  });
  $('gf-art-remove').addEventListener('click', () => {
    artwork = editing ? { action: 'remove' } : { action: 'keep' };
    coverPath = null;
    coverCrop = null;
    renderCover();
  });

  function showError(e) {
    const el = $('gf-error');
    el.textContent = typeof e === 'string' ? e : (e && e.message) || String(e);
    el.hidden = false;
    m.body.scrollTop = 0;
  }

  /** What happens with the artwork: a new image (with the chosen part), only another part, or nothing. */
  function artworkChange() {
    const crop = cropper && cropper.isDirty() ? cropper.getCrop() : null;
    if (artwork.action === 'set') return crop ? { ...artwork, crop } : artwork;
    if (artwork.action === 'keep' && crop && editing) return { action: 'crop', crop };
    return artwork;
  }

  let saving = false;
  async function save() {
    if (saving) return;
    const title = $('gf-title').value.trim();
    const folder = $('gf-folder').value.trim();
    if (!title) { showError('Please enter the game title.'); $('gf-title').focus(); return; }
    if (!folder) { showError('Please choose the game folder.'); $('gf-browse').focus(); return; }
    const yearRaw = $('gf-year').value.trim();
    const input = {
      title,
      folderPath: folder,
      description: $('gf-desc').value,
      developer: $('gf-dev').value,
      publisher: $('gf-pub').value,
      releaseYear: yearRaw ? Number(yearRaw) : null,
      genre: $('gf-genre').value,
      archiveNotes: g.archiveNotes || '',
      tags: $('gf-tags').value.split(',').map((t) => t.trim()).filter(Boolean),
      artwork: artworkChange(),
    };
    saving = true;
    $('gf-save').disabled = true;
    try {
      let id = g.id;
      if (editing) await api.updateGame(g.id, input);
      else id = await api.createGame(input);
      toast(editing ? `Saved “${title}”.` : `Added “${title}” to the catalog.`);
      m.close(id);
    } catch (e) {
      showError(e);
    } finally {
      saving = false;
      $('gf-save') && ($('gf-save').disabled = false);
    }
  }
  $('gf-save').addEventListener('click', save);
  return m.result.then((v) => v ?? null);
}
