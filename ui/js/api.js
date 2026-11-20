// Thin wrapper around Tauri IPC. All file-system work happens in Rust.

const tauri = window.__TAURI__;

if (!tauri || !tauri.core) {
  document.body.innerHTML =
    '<div style="padding:40px;font:16px Tahoma,sans-serif;color:#e4e8f0">Game Curator must be started with GameCurator.exe.</div>';
  throw new Error('Tauri runtime not available');
}

export function call(cmd, args = {}) {
  return tauri.core.invoke(cmd, args);
}

export function listen(event, handler) {
  return tauri.event.listen(event, (e) => handler(e.payload));
}

/** URL of an archive image (artwork/thumbnail) served by the gcimg protocol. */
export function imgSrc(rel) {
  if (!rel) return '';
  return tauri.core.convertFileSrc(rel, 'gcimg');
}

export const api = {
  appInfo: () => call('app_info'),
  adoptDatabase: () => call('adopt_database'),
  archiveStatus: () => call('archive_status'),
  getConfig: () => call('get_config'),
  saveConfig: (config) => call('save_config', { config }),

  listGames: (query) => call('list_games', { query }),
  facets: () => call('library_facets'),
  gameTitles: () => call('game_titles'),
  getGame: (id) => call('get_game', { id }),
  createGame: (input) => call('create_game', { input }),
  updateGame: (id, input) => call('update_game', { id, input }),
  patchGame: (id, patch) => call('patch_game', { id, patch }),
  removalPreview: (id) => call('removal_preview', { id }),
  removeGame: (id, keepJournalCopies, keepScreenshots = true, keepAdditions = true) => call('remove_game', { id, keepJournalCopies, keepScreenshots, keepAdditions }),
  checkAvailability: () => call('check_availability'),

  pickFolder: (title, start) => call('pick_folder', { title, start: start || null }),
  pickArtwork: () => call('pick_artwork'),
  setCoverCrop: (id, crop) => call('set_cover_crop', { id, crop }),

  detectInstalls: () => call('detect_installs'),
  installStatus: (gameId) => call('install_status', { gameId }),
  locateInstall: (gameId) => call('locate_install', { gameId }),
  forgetInstall: (gameId) => call('forget_install', { gameId }),
  openInstallFolder: (gameId) => call('open_install_folder', { gameId }),
  playGame: (gameId) => call('play_game', { gameId, confirmed: true }),

  addScreenshots: (gameId, entryId = null) => call('add_screenshots', { gameId, entryId }),
  addDroppedScreenshots: (gameId, entryId, tokens) => call('add_dropped_screenshots', { gameId, entryId, tokens }),
  listScreenshots: (gameId) => call('list_screenshots', { gameId }),
  setScreenshotCaption: (id, caption) => call('set_screenshot_caption', { id, caption }),
  attachScreenshots: (ids, entryId) => call('attach_screenshots', { ids, entryId }),
  deleteScreenshot: (id) => call('delete_screenshot', { id }),
  openScreenshot: (id) => call('open_screenshot', { id }),

  pickAdditionFile: () => call('pick_addition_file'),
  addAddition: (gameId, token, mode, input) => call('add_addition', { gameId, token, mode, input }),
  listAdditions: (gameId) => call('list_additions', { gameId }),
  updateAddition: (id, input) => call('update_addition', { id, input }),
  removeAddition: (id, deleteCopy) => call('remove_addition', { id, deleteCopy }),
  openAddition: (id) => call('open_addition', { id }),
  runAddition: (id) => call('run_addition', { id, confirmed: true }),
  showAddition: (id) => call('show_addition', { id }),

  archiveContent: (gameId) => call('archive_content', { gameId }),
  setFileCategory: (gameId, fileId, category) => call('set_file_category', { gameId, fileId, category }),
  setFileDisplayName: (gameId, fileId, name) => call('set_file_display_name', { gameId, fileId, name }),
  runningGames: () => call('running_games'),
  setPrimaryFile: (gameId, fileId) => call('set_primary_file', { gameId, fileId }),
  openFile: (gameId, fileId) => call('open_file', { gameId, fileId }),
  runFile: (gameId, fileId) => call('run_file', { gameId, fileId, confirmed: true }),
  openGameFolder: (gameId, subfolder) => call('open_game_folder', { gameId, subfolder: subfolder || null }),

  listJournal: (query) => call('list_journal', { query }),
  createJournal: (input) => call('create_journal', { input }),
  updateJournal: (id, input) => call('update_journal', { id, input }),
  deleteJournal: (id) => call('delete_journal', { id }),

  startScan: (root, depth) => call('start_scan', { root, depth }),
  cancelScan: () => call('cancel_scan'),
  importCandidates: (items) => call('import_candidates', { items }),

  createBackup: () => call('create_backup'),
  listBackups: () => call('list_backups'),
  restoreBackup: (fileName) => call('restore_backup', { fileName }),
  checkWritable: () => call('check_writable'),
};
