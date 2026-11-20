// Built-in PDF viewer (pdf.js). The document is read in parts through Rust
// (`media_read` with a token): the page never sees a file path. PDF scripts
// and forms are never run; pages are drawn on canvases as they scroll into
// view.

import * as pdfjs from '../vendor/pdfjs/pdf.min.mjs';
import { pix } from './pixicons.js';

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

const CHUNK = 512 * 1024;          // pdf.js range chunk size
const PIECE = 8 * CHUNK;           // bytes per read from Rust (4 MB)
const KEEP_RENDERED = 12;          // canvases kept; farther pages are released
const PAD = 16;                    // padding of the page column (CSS: .vw-doc)
const CSS_PER_PT = 96 / 72;        // "100 %" = the printed size
const MAX_CANVAS = 16_777_216;     // pixels per canvas
const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];

const $ = (id) => document.getElementById(id);
const ui = {
  prev: $('vw-prev'), next: $('vw-next'), page: $('vw-page'), pages: $('vw-pages'),
  out: $('vw-out'), zoom: $('vw-zoom'), in: $('vw-in'), external: $('vw-external'),
  scroll: $('vw-scroll'), doc: $('vw-doc'), status: $('vw-status'),
};
ui.prev.innerHTML = pix('up');
ui.next.innerHTML = pix('down');

pdfjs.GlobalWorkerOptions.workerSrc = new URL('../vendor/pdfjs/pdf.worker.min.mjs', import.meta.url).href;
const vendor = (p) => new URL(`../vendor/pdfjs/${p}`, import.meta.url).href;

let doc = null;                    // { token, name, game, size }
let pdf = null;
let zoomMode = 'width';            // 'width' | 'page' | number (1 = 100 %)
let scale = 1;                     // CSS pixels per PDF point
const pages = [];                  // { n, el, w, h, canvas, drawn, stale, task }
let current = 1;
const wanted = new Set();          // pages to draw, nearest first
let drawing = false;

function status(text, isError = false) {
  ui.status.textContent = text;
  ui.status.classList.toggle('error', isError);
}

function fail(e) {
  const msg = String(e?.message || e || 'Unknown error');
  status(msg, true);
}

function mb(bytes) {
  return bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} kB`;
}

async function read(offset, length) {
  return new Uint8Array(await invoke('media_read', { token: doc.token, offset, length }));
}

/** pdf.js asks for byte ranges; they are read from the file by Rust. */
class RustRange extends pdfjs.PDFDataRangeTransport {
  requestDataRange(begin, end) {
    (async () => {
      for (let pos = begin; pos < end; pos += PIECE) {
        const chunk = await read(pos, Math.min(PIECE, end - pos));
        if (!chunk.length) break;
        this.onDataRange(pos, chunk);
      }
    })().catch(fail);
  }
}

// ---------------------------------------------------------------- layout

function fitScale(mode) {
  const first = pages[0];
  if (!first) return 1;
  const cw = Math.max(100, ui.scroll.clientWidth - 2 * PAD);
  const ch = Math.max(100, ui.scroll.clientHeight - 2 * PAD);
  if (mode === 'width') return cw / first.w;
  if (mode === 'page') return Math.min(cw / first.w, ch / first.h);
  return Number(mode) * CSS_PER_PT;
}

function sizePage(p) {
  p.el.style.width = `${Math.round(p.w * scale)}px`;
  p.el.style.height = `${Math.round(p.h * scale)}px`;
}

/** Keeps the same spot of the same page in view while sizes change. */
function anchor() {
  const p = pages[current - 1];
  if (!p) return null;
  return {
    p,
    fy: (ui.scroll.scrollTop - p.el.offsetTop) / Math.max(1, p.el.offsetHeight),
    fx: (ui.scroll.scrollLeft + ui.scroll.clientWidth / 2) / Math.max(1, ui.scroll.scrollWidth),
  };
}

function restore(a) {
  if (!a) return;
  ui.scroll.scrollTop = a.p.el.offsetTop + a.fy * a.p.el.offsetHeight;
  ui.scroll.scrollLeft = a.fx * ui.scroll.scrollWidth - ui.scroll.clientWidth / 2;
}

function applyZoom(mode) {
  zoomMode = mode;
  const a = anchor();
  scale = fitScale(mode);
  for (const p of pages) {
    sizePage(p);
    if (p.drawn) p.stale = true;
  }
  restore(a);
  syncZoomUi();
  schedule();
}

function syncZoomUi() {
  const sel = ui.zoom;
  const custom = sel.querySelector('option[value="custom"]');
  if (typeof zoomMode === 'string') {
    sel.value = zoomMode;
    custom.hidden = true;
  } else {
    const match = [...sel.options].find((o) => Number(o.value) === zoomMode);
    if (match) {
      sel.value = match.value;
      custom.hidden = true;
    } else {
      custom.textContent = `${Math.round(zoomMode * 100)} %`;
      custom.hidden = false;
      sel.value = 'custom';
    }
  }
}

function zoomBy(dir) {
  const now = scale / CSS_PER_PT;
  const next = dir > 0 ? ZOOM_STEPS.find((z) => z > now + 0.001) : [...ZOOM_STEPS].reverse().find((z) => z < now - 0.001);
  if (next) applyZoom(next);
}

// ---------------------------------------------------------------- drawing

function visibleRange() {
  const top = ui.scroll.scrollTop;
  const bottom = top + ui.scroll.clientHeight;
  let first = null;
  let last = null;
  // binary search for the first page whose bottom is below the top edge
  let lo = 0;
  let hi = pages.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    const el = pages[mid].el;
    if (el.offsetTop + el.offsetHeight < top) lo = mid + 1; else hi = mid;
  }
  first = lo;
  last = lo;
  while (last + 1 < pages.length && pages[last + 1].el.offsetTop < bottom) last++;
  return [first, last];
}

function updateCurrent() {
  if (!pages.length) return;
  const mark = ui.scroll.scrollTop + ui.scroll.clientHeight / 3;
  let n = 1;
  let lo = 0;
  let hi = pages.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (pages[mid].el.offsetTop <= mark) {
      n = mid + 1;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  if (n !== current) {
    current = n;
  }
  if (document.activeElement !== ui.page) ui.page.value = String(current);
  ui.prev.disabled = current <= 1;
  ui.next.disabled = current >= pages.length;
}

function schedule() {
  if (!pages.length) return;
  updateCurrent();
  const [first, last] = visibleRange();
  wanted.clear();
  // visible pages first, then one page before and two after
  for (let i = first; i <= last; i++) wanted.add(i);
  for (const i of [last + 1, first - 1, last + 2]) if (i >= 0 && i < pages.length) wanted.add(i);
  for (const i of wanted) {
    const p = pages[i];
    if (p.drawn && !p.stale) wanted.delete(i);
  }
  release(first, last);
  drawNext();
}

/** Frees canvases far from the visible pages. */
function release(first, last) {
  const drawn = pages.filter((p) => p.drawn);
  if (drawn.length <= KEEP_RENDERED) return;
  const mid = (first + last) / 2;
  drawn.sort((a, b) => Math.abs(b.n - 1 - mid) - Math.abs(a.n - 1 - mid));
  for (const p of drawn.slice(0, drawn.length - KEEP_RENDERED)) {
    if (p.n - 1 >= first - 1 && p.n - 1 <= last + 2) continue;
    p.canvas?.remove();
    p.canvas = null;
    p.drawn = false;
    p.stale = false;
    p.el.classList.remove('drawn');
  }
}

async function drawNext() {
  if (drawing) return;
  const i = wanted.values().next().value;
  if (i === undefined) return;
  wanted.delete(i);
  drawing = true;
  try {
    await draw(pages[i]);
  } catch (e) {
    if (e?.name !== 'RenderingCancelledException') {
      pages[i].el.dataset.error = 'This page could not be shown.';
      console.warn(e);
    }
  } finally {
    drawing = false;
    drawNext();
  }
}

async function draw(p) {
  const page = await pdf.getPage(p.n);
  const base = page.getViewport({ scale: 1 });
  if (Math.abs(base.width - p.w) > 0.5 || Math.abs(base.height - p.h) > 0.5) {
    // pages are laid out with the first page's size until they are drawn
    const a = anchor();
    p.w = base.width;
    p.h = base.height;
    sizePage(p);
    restore(a);
  }
  const dpr = window.devicePixelRatio || 1;
  let s = scale * dpr;
  const area = base.width * s * base.height * s;
  if (area > MAX_CANVAS) s *= Math.sqrt(MAX_CANVAS / area);
  const viewport = page.getViewport({ scale: s });
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.floor(viewport.width));
  canvas.height = Math.max(1, Math.floor(viewport.height));
  const ctx = canvas.getContext('2d', { alpha: false });
  const drawnAt = scale;
  p.task = page.render({ canvasContext: ctx, canvas, viewport, annotationMode: pdfjs.AnnotationMode.ENABLE });
  await p.task.promise;
  p.task = null;
  p.canvas?.remove();
  p.canvas = canvas;
  p.el.appendChild(canvas);
  p.el.classList.add('drawn');
  delete p.el.dataset.error;
  p.drawn = true;
  p.stale = drawnAt !== scale;
  page.cleanup();
}

// ---------------------------------------------------------------- navigation

function goTo(n) {
  if (!pages.length) return;
  n = Math.min(pages.length, Math.max(1, Math.round(n) || 1));
  ui.scroll.scrollTop = pages[n - 1].el.offsetTop - PAD / 2;
  current = n;
  ui.page.value = String(n);
  schedule();
}

ui.prev.addEventListener('click', () => goTo(current - 1));
ui.next.addEventListener('click', () => goTo(current + 1));
ui.page.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    goTo(Number(ui.page.value));
    ui.scroll.focus();
  } else if (e.key === 'Escape') {
    ui.page.value = String(current);
    ui.scroll.focus();
  }
});
ui.page.addEventListener('blur', () => { ui.page.value = String(current); });
ui.page.addEventListener('focus', () => ui.page.select());
ui.zoom.addEventListener('change', () => {
  const v = ui.zoom.value;
  if (v === 'custom') return;
  applyZoom(v === 'width' || v === 'page' ? v : Number(v));
  ui.scroll.focus();
});
ui.out.addEventListener('click', () => zoomBy(-1));
ui.in.addEventListener('click', () => zoomBy(1));
ui.external.addEventListener('click', () => {
  if (!doc) return;
  invoke('media_open_external', { token: doc.token })
    .then(() => status('Opened in the default application.'))
    .catch(fail);
});

let scrollQueued = false;
ui.scroll.addEventListener('scroll', () => {
  if (scrollQueued) return;
  scrollQueued = true;
  requestAnimationFrame(() => {
    scrollQueued = false;
    schedule();
  });
});

new ResizeObserver(() => {
  if (pages.length && typeof zoomMode === 'string') applyZoom(zoomMode);
  else schedule();
}).observe(ui.scroll);

ui.scroll.addEventListener('wheel', (e) => {
  if (!e.ctrlKey) return;
  e.preventDefault();
  zoomBy(e.deltaY < 0 ? 1 : -1);
}, { passive: false });

/** Pages wider than the window: the arrow keys scroll sideways instead. */
const wide = () => ui.scroll.scrollWidth > ui.scroll.clientWidth + 1;

document.addEventListener('keydown', (e) => {
  if (!pdf || e.target === ui.page || e.target === ui.zoom) return;
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && (e.key === '+' || e.key === '=')) { e.preventDefault(); zoomBy(1); }
  else if (ctrl && e.key === '-') { e.preventDefault(); zoomBy(-1); }
  else if (ctrl && e.key === '0') { e.preventDefault(); applyZoom('width'); }
  else if (e.key === 'PageDown' || (e.key === 'ArrowRight' && !ctrl && !wide())) { e.preventDefault(); goTo(current + 1); }
  else if (e.key === 'PageUp' || (e.key === 'ArrowLeft' && !ctrl && !wide())) { e.preventDefault(); goTo(current - 1); }
  else if (e.key === 'Home' && ctrl) { e.preventDefault(); goTo(1); }
  else if (e.key === 'End' && ctrl) { e.preventDefault(); goTo(pages.length); }
  else if (ctrl && e.key.toLowerCase() === 'g') { e.preventDefault(); ui.page.focus(); }
});

// an app window, not a web page: no browser context menu, dropped files are not opened
document.addEventListener('contextmenu', (e) => {
  if (!e.target.closest('input, textarea')) e.preventDefault();
});
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => e.preventDefault());

// the theme follows the main window
window.addEventListener('storage', (e) => {
  if (e.key !== 'gc-theme' && e.key !== 'gc-classic-font') return;
  const root = document.documentElement;
  const theme = localStorage.getItem('gc-theme') || 'classic';
  root.setAttribute('data-theme', theme);
  root.classList.toggle('term', theme !== 'classic');
  root.setAttribute('data-classic-font', localStorage.getItem('gc-classic-font') || 'pixel-operator');
});

// ---------------------------------------------------------------- start

async function open() {
  doc = await invoke('viewer_document');
  document.title = `${doc.name} \u2014 ${doc.game}`;
  if (!doc.size) throw new Error('The file is empty.');
  status(`Loading ${mb(doc.size)}…`);
  const initial = await read(0, Math.min(doc.size, 2 * CHUNK));
  const task = pdfjs.getDocument({
    range: new RustRange(doc.size, initial),
    rangeChunkSize: CHUNK,
    isEvalSupported: false,
    enableXfa: false,
    standardFontDataUrl: vendor('standard_fonts/'),
    wasmUrl: vendor('wasm/'),
    iccUrl: vendor('iccs/'),
  });
  task.onPassword = () => {
    task.destroy();
    fail('This PDF is protected by a password. Use “Open in default app” to read it.');
  };
  pdf = await task.promise;
  const first = (await pdf.getPage(1)).getViewport({ scale: 1 });
  const frag = document.createDocumentFragment();
  for (let n = 1; n <= pdf.numPages; n++) {
    const el = document.createElement('div');
    el.className = 'vw-page';
    el.dataset.page = String(n);
    pages.push({ n, el, w: first.width, h: first.height, canvas: null, drawn: false, stale: false, task: null });
    frag.appendChild(el);
  }
  ui.doc.appendChild(frag);
  ui.pages.textContent = String(pdf.numPages);
  for (const b of [ui.page, ui.zoom, ui.out, ui.in]) b.disabled = false;
  status(`${pdf.numPages} ${pdf.numPages === 1 ? 'page' : 'pages'} · ${mb(doc.size)}`);
  applyZoom('width');
  ui.scroll.focus();
}

open().catch((e) => {
  console.error(e);
  const msg = e?.name === 'InvalidPDFException'
    ? 'This file is not a readable PDF document. Use “Open in default app” to try another program.'
    : e;
  fail(msg);
});
