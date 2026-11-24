// Add Game / Edit Game form (modal window).

import { api, imgSrc } from './api.js';
import { esc, openModal, toast, initials } from './ui.js';
import { state } from './store.js';
import { createCropper } from './cropper.js';
import { t } from './i18n.js';

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
        <label class="req" for="gf-title">${t('Title')}</label>
        <input class="field" id="gf-title" maxlength="200" value="${esc(g.title || '')}" autofocus>
        <label class="req" for="gf-folder" style="align-self:start;margin-top:6px">${t('Game folder')}</label>
        <div><div class="row"><input class="field mono" id="gf-folder" value="${esc(g.folderResolved || '')}" placeholder="${esc(t('Folder with the installer files'))}">
          <button class="btn" id="gf-browse" type="button">${t('Browse…')}</button>
          <button class="btn help-toggle" id="gf-folder-help-btn" type="button" aria-expanded="false" title="${esc(t('How to choose the game folder'))}">?</button></div>
          <div class="help-box" id="gf-folder-help" hidden>
            <b>${t('The game folder')}</b>
            <ul>
              <li>${t('Choose the folder that holds <b>this one game</b>: its installer (<span class="mono">setup_….exe</span> and <span class="mono">.bin</span> parts), patches, manuals, soundtrack and extras.')}</li>
              <li>${t('Subfolders are fine (e.g. <span class="mono">Extras</span>, <span class="mono">Patches</span>) – Game Curator lists their files too.')}</li>
              <li>${t('Not the top folder of the whole collection – for that use the <b>Archive Scanner</b>, it finds the game folders for you.')}</li>
              <li>${t('A folder on the archive drive is stored relative to Game Curator, so it keeps working under another drive letter.')}</li>
            </ul>
          </div></div>
        <label for="gf-dev">${t('Developer')}</label><input class="field" id="gf-dev" maxlength="200" value="${esc(g.developer || '')}">
        <label for="gf-pub">${t('Publisher')}</label><input class="field" id="gf-pub" maxlength="200" value="${esc(g.publisher || '')}">
        <label for="gf-year">${t('Release year')}</label>
        <div class="row"><input class="field" id="gf-year" type="number" min="1940" max="2100" style="max-width:110px;flex:none" value="${g.releaseYear ?? ''}">
          <label for="gf-genre" class="muted" style="margin-left:12px">${t('Genre')}</label>
          <input class="field" id="gf-genre" maxlength="100" autocomplete="off" placeholder="${esc(t('e.g. Role-playing, Strategy'))}" value="${esc(g.genre || '')}"></div>
        <label for="gf-tags">${t('Tags')}</label>
        <input class="field" id="gf-tags" value="${esc((g.tags || []).join(', '))}" placeholder="${esc(t('Comma separated, e.g. GOG, big box, Infinity Engine'))}">
        <div></div><div class="hint">${facets.tags.length ? t('Existing tags: {tags}', { tags: facets.tags.slice(0, 14).map((x) => esc(x.name)).join(', ') }) : t('Tags help you group games your own way.')}</div>
        <label for="gf-desc" style="align-self:start;margin-top:4px">${t('Description')}</label>
        <textarea class="field" id="gf-desc" rows="5" maxlength="20000">${esc(g.description || '')}</textarea>
      </div>
      <div>
        <div id="gf-cover"></div>
        <div style="display:flex;flex-direction:column;gap:6px;margin-top:8px">
          <button class="btn" id="gf-art" type="button">${t('Choose image…')}</button>
          <button class="btn small" id="gf-art-remove" type="button">${t('Remove image')}</button>
          <div class="hint" id="gf-art-hint">${t('JPG, PNG or WebP. An optimized WebP copy is stored in the archive.')}</div>
        </div>
      </div>
    </div>
    <p class="hint" style="margin:10px 0 0">${t('Only the title and the folder are required. Game Curator never moves, changes or deletes files in the game folder.')}</p>`;

  const m = openModal({
    title: editing ? t('Edit game – {title}', { title: g.title }) : t('Add Game'),
    width: 840,
    body,
    footer: `<button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="gf-save">${editing ? t('Save changes') : t('Add to catalog')}</button>`,
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
      $('gf-art-hint').textContent = t('Drag the picture to choose what the card shows; zoom with the wheel or the slider.');
    } else {
      cropper = null;
      box.innerHTML = `<div class="cover bevel-out" style="width:200px"><div class="ph" style="position:absolute;inset:0">${esc(initials(title))}<span></span></div></div>`;
      $('gf-art-hint').textContent = t('JPG, PNG or WebP. An optimized WebP copy is stored in the archive.');
    }
    $('gf-art-remove').disabled = !coverPath;
  }
  renderCover();
  $('gf-title').addEventListener('input', () => { if (!coverPath) renderCover(); });

  $('gf-folder-help-btn').addEventListener('click', () => {
    const box = $('gf-folder-help');
    box.hidden = !box.hidden;
    $('gf-folder-help-btn').setAttribute('aria-expanded', String(!box.hidden));
  });
  $('gf-browse').addEventListener('click', async () => {
    try {
      const start = $('gf-folder').value || (state.config && state.config.scanRoot) || (state.info && state.info.gamesDir);
      const picked = await api.pickFolder(t('Choose the game folder'), start);
      if (picked) {
        $('gf-folder').value = picked;
        if (!$('gf-title').value.trim()) { $('gf-title').value = titleFromFolder(picked); renderCover(); }
      }
    } catch (e) { showError(e); }
  });
  $('gf-art').addEventListener('click', async () => {
    const btn = $('gf-art');
    btn.disabled = true;
    btn.textContent = t('Processing…');
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
      btn.textContent = t('Choose image…');
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
    if (!title) { showError(t('Please enter the game title.')); $('gf-title').focus(); return; }
    if (!folder) { showError(t('Please choose the game folder.')); $('gf-browse').focus(); return; }
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
      tags: $('gf-tags').value.split(',').map((x) => x.trim()).filter(Boolean),
      artwork: artworkChange(),
    };
    saving = true;
    $('gf-save').disabled = true;
    try {
      let id = g.id;
      if (editing) await api.updateGame(g.id, input);
      else id = await api.createGame(input);
      toast(editing ? t('Saved “{title}”.', { title }) : t('Added “{title}” to the catalog.', { title }));
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
