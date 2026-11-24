// Entry point: loads the archive info, wires navigation, status bar and
// keyboard shortcuts, then shows the start screen.

import { initSessions } from './sessions.js';
import { api } from './api.js';
import { esc, status, alertBox, confirmBox, modalOpen, toast } from './ui.js';
import { t, tn, tm, setLang, applyStatic } from './i18n.js';
import { state, views, navigate, onNavigate, applyTheme, applyClassicFont } from './store.js';
import { refreshFacets } from './facets.js';
import { libraryView } from './library.js';
import { profileView } from './profile.js';
import { journalView } from './journal.js';
import { scannerView, initScannerEvents } from './scanner.js';
import { initGogEvents } from './gogdetails.js';
import { settingsView } from './settings.js';
import { aboutView } from './about.js';
import { openWizard } from './wizard.js';
import { openWelcome } from './welcome.js';
import { initDropzones } from './dropzone.js';

Object.assign(views, {
  library: libraryView,
  profile: profileView,
  journal: journalView,
  scanner: scannerView,
  settings: settingsView,
  about: aboutView,
});

function setArchiveIndicator(ok, root) {
  state.archiveOk = ok;
  const el = document.getElementById('sb-archive');
  el.className = `sb-archive${ok ? '' : ' bad'}`;
  el.innerHTML = ok
    ? `<span class="led on"></span> ${t('Archive: {root}', { root: esc(root) })}`
    : `<span class="led off"></span> ${t('ARCHIVE DRIVE NOT AVAILABLE – reconnect the drive')}`;
  el.title = root;
}

let lastOk = true;
async function pollArchive() {
  try {
    const s = await api.archiveStatus();
    setArchiveIndicator(s.available, s.root);
    if (s.available && !lastOk) {
      toast(t('The archive drive is available again.'));
      await refreshFacets();
      if (state.view === 'library') libraryView.reload();
    } else if (!s.available && lastOk) {
      toast(t('The archive drive was disconnected. Nothing has been deleted – reconnect the drive to continue.'), 'err');
    }
    lastOk = s.available;
  } catch (_) { /* ignore */ }
}

async function checkAvailabilityInBackground() {
  try {
    const r = await api.checkAvailability();
    if (r.unavailable > 0) {
      status(tn(r.unavailable, '{n} game folder is not available (drive disconnected?). Its record is kept.', '{n} game folders are not available (drive disconnected?). Their records are kept.'), 'err');
    }
    // folders may also have come back since the last start
    if (!state.facets || state.facets.unavailable !== r.unavailable) {
      await refreshFacets();
      if (state.view === 'library') libraryView.reload();
    }
  } catch (_) { /* reported by the archive indicator */ }
}

async function handleIdMismatch() {
  const s = state.info.idStatus;
  const adopt = await confirmBox({
    title: t('Database from another archive'),
    html: t('The database <span class="mono">Data/library.db</span> belongs to a different Game Curator archive.\n\nThis happens when the database file was copied here from another archive.\n\nDatabase archive ID: <span class="mono">{db}</span>\nThis archive ID: <span class="mono">{file}</span>\n\nUse this database for this archive? (Nothing is deleted either way.)', { db: esc(s.dbId), file: esc(s.fileId) }),
    ok: t('Use it for this archive'),
    cancel: t('Decide later'),
    width: 560,
  });
  if (adopt) {
    await api.adoptDatabase();
    state.info = await api.appInfo();
    toast(t('The database now belongs to this archive.'));
  } else {
    status(t('Warning: the database belongs to another archive (see Settings).'), 'err');
  }
}

function initChrome() {
  document.getElementById('nav').addEventListener('click', (e) => {
    const b = e.target.closest('[data-view]');
    if (b) navigate(b.dataset.view);
  });
  onNavigate((name) => {
    const active = name === 'profile' ? 'library' : name;
    document.querySelectorAll('#nav [data-view]').forEach((b) => b.classList.toggle('active', b.dataset.view === active));
  });
  document.addEventListener('keydown', (e) => {
    if (modalOpen()) return;
    if (e.altKey && ['1', '2', '3', '4', '5'].includes(e.key)) {
      e.preventDefault();
      navigate(['library', 'journal', 'scanner', 'settings', 'about'][Number(e.key) - 1]);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      if (state.view !== 'library') navigate('library').then(() => libraryView.focusSearch());
      else libraryView.focusSearch();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n' && state.view === 'library') {
      e.preventDefault();
      document.getElementById('lib-add')?.click();
    }
  });
  // Block the browser context menu and file drops (this is an app, not a web page).
  document.addEventListener('contextmenu', (e) => {
    if (!e.target.closest('input, textarea, [contenteditable=true]')) e.preventDefault();
  });
  // Artwork file deleted outside the app: fall back to the placeholder cover.
  document.addEventListener('error', (e) => {
    const img = e.target;
    if (!(img instanceof HTMLImageElement) || img.dataset.fallback) return;
    img.dataset.fallback = '1';
    const ph = document.createElement('div');
    ph.className = 'ph';
    ph.textContent = '?';
    ph.title = t('Artwork file not found');
    img.replaceWith(ph);
  }, true);
  document.addEventListener('dragover', (e) => e.preventDefault());
  document.addEventListener('drop', (e) => e.preventDefault());
  // Settings switched the language: relabel the parts outside the views.
  document.addEventListener('gc-lang', () => {
    applyStatic();
    setArchiveIndicator(state.archiveOk, state.info?.archiveRoot || '');
    if (state.info) document.getElementById('sb-version').textContent = `v${state.info.version}${state.info.testMode ? t(' (test build)') : ''}`;
    refreshFacets();
  });
  window.addEventListener('focus', pollArchive);
  setInterval(pollArchive, 30000);
}

async function init() {
  initChrome();
  initScannerEvents();
  initGogEvents();
  try {
    [state.info, state.config] = await Promise.all([api.appInfo(), api.getConfig()]);
  } catch (e) {
    setArchiveIndicator(false, '');
    await alertBox(t('The archive could not be loaded:\n\n{error}', { error: tm(e) }), { kind: 'err', title: 'Game Curator' });
    return;
  }
  setLang(state.config.language);
  applyTheme(state.config.theme);
  applyClassicFont(state.config.classicFont);
  state.query.sort = state.config.sort;
  document.getElementById('sb-version').textContent = `v${state.info.version}${state.info.testMode ? t(' (test build)') : ''}`;
  setArchiveIndicator(true, state.info.archiveRoot);
  // which games are installed on this PC (registry, read-only; milliseconds)
  await api.detectInstalls().catch(() => null);
  await refreshFacets();
  await navigate('library');
  if (state.info.startupNotes.length) status(state.info.startupNotes.map((n) => tm(n)).join(' '), 'ok');
  initDropzones();
  await initSessions();
  if (state.info.idStatus.state === 'mismatch') await handleIdMismatch();
  if (state.config.showWelcome !== false) await openWelcome();
  if (!state.config.wizardCompleted) await openWizard(false);
  checkAvailabilityInBackground();
}

init();
