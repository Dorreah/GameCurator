// Artwork crop editor. The frame has the shape of the card (3:4); the user
// drags the picture to choose what is shown and zooms with the wheel or the
// slider - from "fit the whole picture" up to 5x. The result is a crop in
// fractions of the picture, which the core renders into the thumbnail.

import { t } from './i18n.js';

const MAX_ZOOM = 5;

/**
 * Builds the editor inside `host`.
 * @returns {{ getCrop(): ({x:number,y:number,w:number,h:number}|null), isDirty(): boolean, ready: Promise<void> }}
 */
export function createCropper(host, { src, crop = null, width = 200, onChange = () => {} }) {
  const fw = width;
  const fh = Math.round((width * 4) / 3);
  host.innerHTML = `<div class="cropper">
    <div class="crop-frame" style="width:${fw}px;height:${fh}px" tabindex="0"
      title="${t('Drag to choose the part shown on the card · scroll to zoom · double-click to fill the frame')}">
      <img alt="" draggable="false"><div class="crop-grid" aria-hidden="true"></div>
    </div>
    <div class="crop-tools" style="width:${fw}px">
      <span class="crop-sign" aria-hidden="true">−</span>
      <input type="range" class="crop-zoom" min="0" max="1000" step="1" value="0" aria-label="${t('Zoom')}">
      <span class="crop-sign" aria-hidden="true">+</span>
    </div>
    <div class="crop-tools" style="width:${fw}px">
      <button class="btn small" type="button" data-fit title="${t('Show the whole picture')}">${t('Fit')}</button>
      <button class="btn small" type="button" data-fill title="${t('Fill the frame, centred')}">${t('Fill')}</button>
    </div></div>`;
  const frame = host.querySelector('.crop-frame');
  const img = host.querySelector('img');
  const slider = host.querySelector('.crop-zoom');

  let W = 1; let H = 1; let s = 1; let ox = 0; let oy = 0;
  let sMin = 1; let sMax = 1; let sCover = 1; let sContain = 1;
  let ready = false;
  let dirty = false;

  const clampRange = (v, a, b) => Math.min(Math.max(v, Math.min(a, b)), Math.max(a, b));

  function clamp() {
    s = Math.min(Math.max(s, sMin), sMax);
    // a picture larger than the frame never shows a gap; a smaller one stays inside
    ox = clampRange(ox, 0, fw - W * s);
    oy = clampRange(oy, 0, fh - H * s);
  }
  function apply() {
    img.style.width = `${W * s}px`;
    img.style.height = `${H * s}px`;
    img.style.transform = `translate(${ox}px, ${oy}px)`;
    slider.value = String(Math.round((1000 * Math.log(s / sMin)) / Math.log(sMax / sMin || 2)));
  }
  function changed() {
    dirty = true;
    onChange();
  }
  function centre(scale) {
    s = scale;
    ox = (fw - W * s) / 2;
    oy = (fh - H * s) / 2;
    clamp();
    apply();
  }
  function zoomAt(next, cx = fw / 2, cy = fh / 2) {
    const px = (cx - ox) / s;
    const py = (cy - oy) / s;
    s = Math.min(Math.max(next, sMin), sMax);
    ox = cx - px * s;
    oy = cy - py * s;
    clamp();
    apply();
    changed();
  }

  const loaded = new Promise((resolve) => {
    img.addEventListener('load', () => {
      W = img.naturalWidth || 1;
      H = img.naturalHeight || 1;
      sCover = Math.max(fw / W, fh / H);
      sContain = Math.min(fw / W, fh / H);
      sMin = sContain;
      sMax = sCover * MAX_ZOOM;
      if (crop && crop.w > 0 && crop.h > 0) {
        s = fw / (crop.w * W);
        ox = -crop.x * s * W;
        oy = -crop.y * s * H;
        clamp();
        apply();
      } else {
        centre(sCover);
      }
      ready = true;
      frame.classList.add('ready');
      resolve();
    }, { once: true });
    img.addEventListener('error', () => resolve(), { once: true });
  });
  img.src = src;

  // drag
  let drag = null;
  frame.addEventListener('pointerdown', (e) => {
    if (!ready || e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, ox, oy };
    try { frame.setPointerCapture(e.pointerId); } catch (_) { /* synthetic or finished pointer */ }
    frame.classList.add('dragging');
    e.preventDefault();
  });
  frame.addEventListener('pointermove', (e) => {
    if (!drag) return;
    ox = drag.ox + (e.clientX - drag.x);
    oy = drag.oy + (e.clientY - drag.y);
    clamp();
    apply();
  });
  const end = (e) => {
    if (!drag) return;
    const moved = e.clientX !== drag.x || e.clientY !== drag.y;
    drag = null;
    frame.classList.remove('dragging');
    if (moved) changed();
  };
  frame.addEventListener('pointerup', end);
  frame.addEventListener('pointercancel', end);

  // zoom
  frame.addEventListener('wheel', (e) => {
    if (!ready) return;
    e.preventDefault();
    const r = frame.getBoundingClientRect();
    zoomAt(s * Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top);
  }, { passive: false });
  slider.addEventListener('input', () => {
    if (!ready) return;
    zoomAt(sMin * (sMax / sMin) ** (Number(slider.value) / 1000));
  });
  frame.addEventListener('dblclick', () => { if (ready) { centre(sCover); changed(); } });
  host.querySelector('[data-fit]').addEventListener('click', () => { if (ready) { centre(sContain); changed(); } });
  host.querySelector('[data-fill]').addEventListener('click', () => { if (ready) { centre(sCover); changed(); } });
  frame.addEventListener('keydown', (e) => {
    if (!ready) return;
    const step = e.shiftKey ? 20 : 4;
    const moves = { ArrowLeft: [step, 0], ArrowRight: [-step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
    if (moves[e.key]) {
      e.preventDefault();
      ox += moves[e.key][0];
      oy += moves[e.key][1];
      clamp();
      apply();
      changed();
    } else if (e.key === '+' || e.key === '=') {
      e.preventDefault();
      zoomAt(s * 1.1);
    } else if (e.key === '-') {
      e.preventDefault();
      zoomAt(s / 1.1);
    }
  });

  return {
    getCrop() {
      if (!ready) return null;
      const r = (v) => Math.round(v * 1e6) / 1e6;
      return { x: r(-ox / (s * W)), y: r(-oy / (s * H)), w: r(fw / (s * W)), h: r(fh / (s * H)) };
    },
    isDirty: () => dirty,
    ready: loaded,
  };
}
