// Files dragged from Explorer. Rust reports where the pointer is (gc-drag)
// and, on drop, the files with one-time tokens (gc-drop). Any element with
// `data-dropzone="<hint>"` can take them: it receives a `gc-files` DOM event
// with `detail.files` and sets `detail.handled = true`.

import { listen } from './api.js';
import { toast } from './ui.js';
import { t, tn } from './i18n.js';

let current = null;

function zoneAt(x, y) {
  const el = document.elementFromPoint(x, y);
  return el ? el.closest('[data-dropzone]') : null;
}

function highlight(zone) {
  if (current === zone) return;
  if (current) current.classList.remove('drop-over');
  current = zone;
  if (zone) zone.classList.add('drop-over');
}

export function initDropzones() {
  const root = document.documentElement;
  listen('gc-drag', (p) => {
    if (p.kind === 'leave') {
      root.classList.remove('dragging-files');
      highlight(null);
      return;
    }
    root.classList.add('dragging-files');
    highlight(zoneAt(p.x, p.y));
  });
  listen('gc-drop', (p) => {
    root.classList.remove('dragging-files');
    const zone = zoneAt(p.x, p.y);
    highlight(null);
    if (!p.files.length) {
      toast(t('Only files can be dropped (not folders).'), 'err');
      return;
    }
    if (!zone) {
      toast(t('Drop files on “Screenshots” or “Additional files” in the profile of a game.'), 'err');
      return;
    }
    const detail = { files: p.files, handled: false };
    zone.dispatchEvent(new CustomEvent('gc-files', { detail }));
    if (!detail.handled) toast(t('These files cannot be added here.'), 'err');
    else if (p.skipped) toast(tn(p.skipped, '{n} folder left out – only files can be added.', '{n} folders left out – only files can be added.'));
  });
}

/** Lets `el` take dropped files: `handler(files)` runs (inside guard by the caller). */
export function onFiles(el, hint, handler) {
  if (!el) return;
  el.dataset.dropzone = hint;
  el.addEventListener('gc-files', (e) => {
    e.detail.handled = true;
    e.stopPropagation();
    handler(e.detail.files);
  });
}
