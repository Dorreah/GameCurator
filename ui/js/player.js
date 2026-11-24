// Minimal music player for a game's soundtrack. Tracks are read through
// Rust (`media_read` with a token, never a path) into memory and played from
// there, so seeking is instant even on a slow USB drive.

import { pix } from './pixicons.js';
import { t, tn, tm, mark, applyStatic, onLangChange } from './i18n.js';

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

const PIECE = 8 * 1024 * 1024;           // bytes per read from Rust
const MAX_TRACK = 1024 * 1024 * 1024;    // larger files: default application
const MIME = { mp3: 'audio/mpeg', ogg: 'audio/ogg', oga: 'audio/ogg', opus: 'audio/ogg', flac: 'audio/flac', wav: 'audio/wav', m4a: 'audio/mp4', aac: 'audio/aac' };
const REPEAT = ['off', 'all', 'one'];

const $ = (id) => document.getElementById(id);
const ui = {
  game: $('pl-game'), pos: $('pl-pos'), title: $('pl-title'), seek: $('pl-seek'), time: $('pl-time'), dur: $('pl-dur'), msg: $('pl-msg'),
  prev: $('pl-prev'), play: $('pl-play'), next: $('pl-next'), mute: $('pl-mute'), vol: $('pl-vol'), repeat: $('pl-repeat'),
  list: $('pl-list'), count: $('pl-count'), external: $('pl-external'), audio: $('pl-audio'),
};
const audio = ui.audio;
ui.prev.innerHTML = pix('prev');
ui.next.innerHTML = pix('next');

const store = {
  get(k, d) { try { return localStorage.getItem(k) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* storage unavailable */ } },
};

let playlist = null;       // { game, tracks: [{token, name, size, ext}], start, seq }
let index = -1;
let loadSeq = 0;
let objectUrl = null;
let seeking = false;
let repeat = REPEAT.includes(store.get('gc-player-repeat', 'off')) ? store.get('gc-player-repeat', 'off') : 'off';

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function clock(sec) {
  if (!Number.isFinite(sec) || sec < 0) return '0:00';
  const s = Math.floor(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${r}` : `${m}:${r}`;
}

// the message is kept as a function, so it can be shown again in another
// language when the main window switches it
let messageFn = () => '';
let messageErr = false;
function message(text, isError = false) {
  messageFn = typeof text === 'function' ? text : () => text;
  messageErr = isError;
  ui.msg.textContent = messageFn();
  ui.msg.classList.toggle('error', isError);
}

function playIcon() {
  const playing = !audio.paused && !audio.ended;
  ui.play.innerHTML = pix(playing ? 'pause' : 'play', 27);
  ui.play.title = playing ? t('Pause (Space)') : t('Play (Space)');
}

function volumeUi() {
  ui.vol.value = String(audio.volume);
  ui.mute.innerHTML = pix(audio.muted || audio.volume === 0 ? 'mute' : 'volume');
  ui.mute.title = audio.muted ? t('Sound on') : t('Mute');
  ui.mute.classList.toggle('pressed', audio.muted);
}

function repeatUi() {
  ui.repeat.innerHTML = pix('repeat') + (repeat === 'one' ? '<span class="pl-one">1</span>' : '');
  ui.repeat.classList.toggle('pressed', repeat !== 'off');
  ui.repeat.title = t({ off: mark('Repeat: off'), all: mark('Repeat: all tracks'), one: mark('Repeat: this track') }[repeat]);
}

function renderList() {
  const tracks = playlist?.tracks || [];
  const digits = String(tracks.length).length < 2 ? 2 : String(tracks.length).length;
  // "01 - Main Theme": the names already carry their numbers
  const numbered = tracks.length > 0 && tracks.every((trk) => /^\d/.test(trk.name));
  ui.list.classList.toggle('numbered', numbered);
  ui.list.innerHTML = tracks.map((trk, i) => `
    <li><button class="pl-track${i === index ? ' on' : ''}" data-i="${i}" title="${esc(trk.name)}">
      <span class="pl-mark">${i === index ? pix('note', 18) : ''}</span>
      ${numbered ? '' : `<span class="pl-num">${String(i + 1).padStart(digits, '0')}</span>`}
      <span class="pl-name">${esc(trk.name)}</span>
    </button></li>`).join('');
  ui.count.textContent = tracks.length ? tn(tracks.length, '{n} track', '{n} tracks') : '';
  ui.list.querySelector('.pl-track.on')?.scrollIntoView({ block: 'nearest' });
}

function headerUi() {
  const trk = playlist?.tracks[index];
  ui.game.textContent = playlist ? playlist.game : t('Music');
  ui.title.textContent = trk ? trk.name : ' ';
  ui.pos.textContent = trk ? `${index + 1} / ${playlist.tracks.length}` : '';
  ui.external.disabled = !trk;
  document.title = playlist ? `${playlist.game} \u2014 ${t('Music')}` : `${t('Music')} \u2014 Game Curator`;
  if ('mediaSession' in navigator && trk) {
    try {
      navigator.mediaSession.metadata = new MediaMetadata({ title: trk.name, album: playlist.game, artist: playlist.game });
    } catch { /* not supported */ }
  }
}

async function readTrack(trk, seq) {
  const parts = [];
  let loaded = 0;
  while (loaded < trk.size) {
    const buf = await invoke('media_read', { token: trk.token, offset: loaded, length: Math.min(PIECE, trk.size - loaded) });
    if (seq !== loadSeq) return null;
    if (!buf.byteLength) break;
    parts.push(buf);
    loaded += buf.byteLength;
    if (trk.size > PIECE) { const pct = Math.round((loaded / trk.size) * 100); message(() => t('Loading {pct} %', { pct })); }
  }
  return { blob: new Blob(parts, { type: MIME[trk.ext] || 'audio/mpeg' }), loaded };
}

async function load(i, autoplay = true) {
  if (!playlist || i < 0 || i >= playlist.tracks.length) return;
  const seq = ++loadSeq;
  index = i;
  const trk = playlist.tracks[i];
  audio.pause();
  ui.seek.disabled = true;
  ui.seek.value = '0';
  ui.time.textContent = '0:00';
  ui.dur.textContent = '0:00';
  headerUi();
  renderList();
  if (trk.size > MAX_TRACK) {
    message(() => t('This file is too large to play here.'), true);
    return;
  }
  message(() => t('Loading…'));
  try {
    const got = await readTrack(trk, seq);
    if (!got) return;
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    objectUrl = URL.createObjectURL(got.blob);
    audio.dataset.track = String(i);
    audio.dataset.bytes = String(got.loaded);
    audio.src = objectUrl;
    message('');
    if (autoplay) {
      audio.play().catch((e) => {
        // a decoding problem is reported by the error event
        playIcon();
        if (e?.name === 'NotAllowedError') message(() => t('Press play to start.'));
      });
    }
  } catch (e) {
    if (seq === loadSeq) { const raw = String(e?.message || e); message(() => tm(raw), true); }
  }
}

function step(dir, fromEnd = false) {
  if (!playlist) return;
  const n = playlist.tracks.length;
  let i = index + dir;
  if (i >= n) {
    if (repeat === 'all' || !fromEnd) i = 0; else { playIcon(); return; }
  }
  if (i < 0) i = n - 1;
  load(i);
}

function toggle() {
  if (!audio.src) {
    if (playlist) load(index < 0 ? 0 : index);
    return;
  }
  if (audio.paused || audio.ended) audio.play().catch(() => {});
  else audio.pause();
}

function setPlaylist(p) {
  if (!p || !p.tracks?.length) return;
  if (playlist && p.seq <= playlist.seq) return;
  const start = Math.min(p.start || 0, p.tracks.length - 1);
  const same = playlist && audio.src && start === index
    && p.tracks.length === playlist.tracks.length && p.tracks.every((trk, i) => trk.token === playlist.tracks[i].token);
  playlist = p;
  if (same) {
    // the track that is already playing was opened again: keep playing
    headerUi();
    renderList();
    if (audio.paused) audio.play().catch(() => {});
    return;
  }
  load(start);
}

// ---------------------------------------------------------------- events

ui.play.addEventListener('click', toggle);
ui.prev.addEventListener('click', () => {
  // like a CD player: back to the start of the track first
  if (audio.currentTime > 3) audio.currentTime = 0;
  else step(-1);
});
ui.next.addEventListener('click', () => step(1));
ui.list.addEventListener('click', (e) => {
  const b = e.target.closest('.pl-track');
  if (b) load(Number(b.dataset.i));
});
ui.repeat.addEventListener('click', () => {
  repeat = REPEAT[(REPEAT.indexOf(repeat) + 1) % REPEAT.length];
  store.set('gc-player-repeat', repeat);
  repeatUi();
});
ui.mute.addEventListener('click', () => {
  audio.muted = !audio.muted;
  volumeUi();
});
ui.vol.addEventListener('input', () => {
  audio.volume = Number(ui.vol.value);
  audio.muted = false;
  store.set('gc-player-volume', String(audio.volume));
  volumeUi();
});
ui.seek.addEventListener('input', () => {
  seeking = true;
  ui.time.textContent = clock(Number(ui.seek.value));
});
ui.seek.addEventListener('change', () => {
  seeking = false;
  if (Number.isFinite(audio.duration)) audio.currentTime = Number(ui.seek.value);
});
ui.external.addEventListener('click', () => {
  const trk = playlist?.tracks[index];
  if (!trk) return;
  audio.pause();
  invoke('media_open_external', { token: trk.token }).then(() => message(() => t('Opened in the default application.'))).catch((e) => message(() => tm(String(e)), true));
});

audio.addEventListener('loadedmetadata', () => {
  ui.seek.max = String(Number.isFinite(audio.duration) ? audio.duration : 0);
  ui.seek.disabled = !Number.isFinite(audio.duration);
  ui.dur.textContent = clock(audio.duration);
});
audio.addEventListener('timeupdate', () => {
  if (seeking) return;
  ui.seek.value = String(audio.currentTime);
  ui.time.textContent = clock(audio.currentTime);
});
audio.addEventListener('play', playIcon);
audio.addEventListener('pause', playIcon);
audio.addEventListener('ended', () => {
  if (repeat === 'one') {
    audio.currentTime = 0;
    audio.play().catch(() => {});
  } else {
    step(1, true);
  }
});
audio.addEventListener('error', () => {
  if (!audio.src) return;
  message(() => t('This track cannot be played here. Try “Open in default app”.'), true);
  playIcon();
});

document.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement && e.key !== ' ') return;
  if (e.key === ' ') {
    e.preventDefault();
    // a focused button would also react to the space key when it is released
    if (document.activeElement instanceof HTMLButtonElement) document.activeElement.blur();
    toggle();
  }
  else if (e.key === 'ArrowRight' && Number.isFinite(audio.duration)) { e.preventDefault(); audio.currentTime = Math.min(audio.duration, audio.currentTime + 5); }
  else if (e.key === 'ArrowLeft' && Number.isFinite(audio.duration)) { e.preventDefault(); audio.currentTime = Math.max(0, audio.currentTime - 5); }
});

if ('mediaSession' in navigator) {
  const ms = navigator.mediaSession;
  const on = (action, fn) => { try { ms.setActionHandler(action, fn); } catch { /* not supported */ } };
  on('play', () => audio.play().catch(() => {}));
  on('pause', () => audio.pause());
  on('previoustrack', () => step(-1));
  on('nexttrack', () => step(1));
}

// an app window, not a web page: no browser context menu, dropped files are not opened
document.addEventListener('contextmenu', (e) => {
  if (!e.target.closest('input, textarea')) e.preventDefault();
});
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => e.preventDefault());

window.addEventListener('storage', (e) => {
  if (e.key !== 'gc-theme' && e.key !== 'gc-classic-font') return;
  const root = document.documentElement;
  const theme = localStorage.getItem('gc-theme') || 'classic';
  root.setAttribute('data-theme', theme);
  root.classList.toggle('term', theme !== 'classic');
  root.setAttribute('data-classic-font', localStorage.getItem('gc-classic-font') || 'pixel-operator');
});

// ---------------------------------------------------------------- start

audio.volume = Math.min(1, Math.max(0, Number(store.get('gc-player-volume', '0.8')) || 0));
volumeUi();
repeatUi();
playIcon();
tauri.event.listen('gc-player-load', (e) => setPlaylist(e.payload));
invoke('player_state').then(setPlaylist).catch((e) => message(() => tm(String(e)), true));

// the language follows the main window
onLangChange(() => {
  applyStatic();
  playIcon();
  volumeUi();
  repeatUi();
  renderList();
  headerUi();
  ui.msg.textContent = messageFn();
  ui.msg.classList.toggle('error', messageErr);
});
