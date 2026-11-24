// About: the idea behind the project, plus the facts about this copy.

import { api } from './api.js';
import { esc } from './ui.js';
import { state } from './store.js';
import { openWelcome } from './welcome.js';
import { t, tm } from './i18n.js';

export const aboutView = {
  async show() {
    const info = await api.appInfo().catch(() => state.info);
    if (info) state.info = info;
    const main = document.getElementById('main');
    main.innerHTML = `<div class="view"><div class="strip"><span>${t('About')}</span></div>
    <div class="library-scroll"><div class="about">
      <div class="about-mark" aria-hidden="true"></div>
      <h1>Game Curator</h1>
      <p class="motto">${t('Your games. Your archive. Your history.')}</p>
      <p><button class="btn small" id="ab-welcome">${t('Show the welcome window')}</button></p>

      <p>${t('As interest in DRM-free games grows, external drives get fuller and more lived-in: installers, patches, manuals, soundtracks and extras of more and more games, gathered over the years.')}</p>

      <p>${t('Game Curator is here to help you find your way around all of it, so that you can enjoy your gaming hobby in peace – put everything in order, describe it, and play at your own pace, whenever you feel like it. It is not a backlog, not a to-do list and not a scoreboard. It is an archive, kept on your own drive, so that even after years you know what you have and where it is.')}</p>

      <p>${t('There is nothing to finish here. No progress bars, no "not played yet", no ratings to keep up with. What the app asks for is attention: write down what you know about a game, what you found out, how it felt to play it. Every game has its own <b>What I know about this game</b> and its own <b>Gaming Journal</b>; the Journal page then gathers all your entries in one place.')}</p>

      <p>${t('Like the big boxes games used to come in, the archive is meant to be opened again – to look inside, remember, and add something.')}</p>

      <blockquote class="about-quote">${t('Gaming history deserves careful, loving treatment.')}</blockquote>

      <h2>${t('Principles')}</h2>
      <ul class="principles">
        <li>${t('<b>Portable.</b> The app and all its data live next to your games. Move the drive, plug it into another PC, and everything is there.')}</li>
        <li>${t('<b>Offline.</b> No accounts, no cloud, nothing running in the background. The only time Game Curator goes online is when you press <b>Fetch details</b> on the Archive Scanner page &ndash; and then only to the source you chose: GOG.com, Databáze-her.cz or MobyGames.')}</li>
        <li>${t('<b>Safe.</b> Game files are never deleted, moved or changed. Installers run only after you confirm.')}</li>
        <li>${t('<b>Yours.</b> A plain SQLite database, journal copies as HTML files, backups you can see in a folder.')}</li>
      </ul>

      <h2>${t('This copy')}</h2>
      ${info ? `<dl class="facts">
        <dt>${t('Version')}</dt><dd>${esc(info.name)} ${esc(info.version)}</dd>
        <dt>${t('Archive')}</dt><dd><span class="led ${state.archiveOk ? 'on' : 'off'}"></span> <span class="mono">${esc(info.archiveRoot)}</span> <span class="hint">· ${esc(tm(info.driveKind))}</span></dd>
        <dt>${t('Games')}</dt><dd>${info.gameCount}</dd>
        <dt>${t('Archive ID')}</dt><dd class="mono">${esc(info.archiveId)}</dd>
        <dt>${t('Database')}</dt><dd class="mono">${esc(info.dbPath)}</dd>
        <dt>WebView</dt><dd>${esc(info.webviewVersion)} (${esc(info.platform)})</dd>
      </dl>` : `<p class="muted">${t('Information about this copy is not available.')}</p>`}

      <h2>${t('Credits')}</h2>
      <p class="hint">${t('The Classic look uses the pixel font <b>Pixel Operator</b> by Jayvee Enaguas (CC0, public domain). DOS Midnight uses <b>PxPlus IBM VGA8</b> by VileR from The Ultimate Oldschool PC Font Pack (int10h.org), licensed CC BY-SA 4.0 and included unchanged. Other fonts for the Classic look: <b>Dogica</b> and <b>Dogica Pixel</b> © 2020 Roberto Mocci (SIL Open Font License 1.1, Reserved Font Names Dogica and Dogica Pixel, included unchanged), <b>Geist Pixel</b> © 2026 The Geist Project Authors (SIL Open Font License 1.1) and <b>Roboto Slab</b> (Apache License 2.0).')}</p>
      <p class="hint">${t('Game details come, when you ask for them, from <b>GOG.com</b>, <b>Databáze-her.cz</b> (descriptions under the CC BY-NC-ND 3.0 CZ licence; the source is written under each one) and <b>MobyGames</b> (through its API, with your own key).')}</p>
      <p class="hint">${t('PDF manuals are shown with <b>PDF.js</b> by Mozilla (Apache License 2.0), with its standard fonts (Foxit, BSD-style licence; Liberation, SIL Open Font License 1.1) and image decoders (OpenJPEG, BSD; PDFium JBIG2, BSD; qcms, MIT). The licence texts are in the <span class="mono">Licenses</span> folder next to GameCurator.exe.')}</p>
    </div></div></div>`;
    document.getElementById('ab-welcome').addEventListener('click', () => openWelcome());
  },
};
