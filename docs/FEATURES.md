# Game Curator – feature tour

The [README](../README.md) gives the idea in short; this page goes through everything Game Curator 0.2 does.
[Česky](README.cs.md).

## Features

**Library**
* Cover grid (title, year, size on disk, "installed" checkbox, OFFLINE badge) or a compact catalog table,
  where installed games show their own icon (read from GOG's `goggame-*.ico` or the game's `.exe`, never
  changed). In the Classic look, titles of installed games are green and the others orange.
* Search by title, ignoring diacritics ("zaklinac" finds "Zaklínač").
  Sort by title, year, size or date added.
* **Filter by genre and tags** with the funnel button next to Sort. Every genre of a field like
  "Role-playing, Strategy" counts on its own. A game shows if it has any of the ticked genres and any of
  the ticked tags; the active filters stay visible above the games, each removable with ×.
* No play status, ratings or "unplayed" filters: it's an archive, not a backlog.

**Game profile**
* **Game:** title, developer, publisher, year, genres (comma separated), tags and artwork. A built-in crop editor lets you pan,
  zoom, fit or fill the cover in the 3:4 frame, and you can re-adjust it any time.
* **Installed on this PC:** detects games installed from GOG installers. It reads the GOG and Windows
  "installed programs" registry entries, read-only. **▶ Play** starts the game after a confirmation, and
  **Show location** opens its folder (the path itself stays off the screen, in a tooltip). If a game isn't
  detected, point to its `.exe` with **Locate…**. Installations are remembered per computer, because the
  drive travels and installations don't.
* **What I know about this game:** free notes on where your copy came from, installer versions, edition
  details, facts and stories you dug up.
* **Gaming Journal:** dated, formatted entries with gentle writing prompts. You can attach screenshots
  to an entry. When a game you started with **▶ Play** ends, Game Curator offers within a second to
  write down your impressions while they are fresh, in a small window that stays on top even if you have
  already switched to another program (with "Don't show this again"; Settings can turn it back on). It
  never counts or shows play time.
* **Screenshots:** the three newest at a glance, and a gallery with captions, arrow-key browsing and the
  Windows photo viewer.
* **Archive:** every file in the game's folder, grouped (installers, installer data, manuals, music,
  images, archives…), with Open, Run (after confirmation) and Open folder. The ✎ pencil gives a file a
  clearer name in the catalog; the file on disk keeps its own name.
* **Manuals and soundtracks inside the app:** **Open** on a PDF shows it in a reader window (pages,
  zoom, fit width, keyboard navigation; built on PDF.js, PDF scripts are never run). **Open** on a music
  file (MP3, FLAC, OGG, WAV, M4A…) plays the game's soundtrack in a small player window with play/pause,
  previous/next, seek, volume, repeat and the track list. Both windows have **Open in default app**, and
  Settings can send PDFs and music to the Windows programs instead.
* **Additional files:** compatibility fixes, community patches, mods, bonus material such as a making-of
  documentary. **Copy** a file into the archive (it travels with the drive) or **link** it where it is.
  The ✎ pencil edits its catalog name, kind and note. Music among them joins the game's soundtrack in the
  player.
* A resizable divider between the game column and the archive column.

**Elsewhere**
* **Gaming Journal page:** all entries from all games in one place, grouped by game.
* **Archive Scanner:** finds installer folders, reads titles from `goggame-*.info`, `title.txt` or folder
  names, and lets you review everything before import. It runs in the background, can be cancelled and
  never creates duplicates. A short built-in guide explains which folder to choose, how a game folder is
  recognised and what the search depth means, with an example folder tree; the depth picker shows the
  path it reaches (`Games\…\Game`). The **?** next to *Game folder* in the game form explains the same for a
  single game.
* **Game details from the internet:** on the Archive Scanner page, choose a source and press **Fetch
  details**:
  * **GOG.com** – the official title, description, genre, tags, developer, publisher, release year and,
    for games without one, the cover (in English).
  * **Databáze-her.cz** – a **Czech description**, genre, developer and year. Game Curator reads the
    public game page whose address it makes from the title (`/hry/baldurs-gate-iii/`); the site's search
    is not used. If a game is not found, paste the address of its page into *Search again*, or open the
    site's search in your browser with **Search on Databáze-her.cz ↗**. The descriptions are licensed
    CC BY-NC-ND 3.0 CZ, so the source and licence are written under each one.
  * **MobyGames** – an **English description**, genres, developer, publisher and year through the
    MobyGames API. It needs your own API key (Settings > Game details from the internet); MobyGames
    allows 360 requests an hour, so Game Curator asks at most once a second. The source is written under
    each description.

  You review every game first (sure matches, matches to check, other products found, "Search again",
  **Game page ↗** to open it at the source). Empty fields are ticked, while anything you filled in yourself
  stays unless you tick it; **Replace descriptions that are already filled in** ticks all offered
  descriptions at once, e.g. to swap English descriptions for Czech ones. After an import it offers to
  look up the new games right away.
* **Drag & drop:** drop files from Explorer onto Screenshots, Additional files, the gallery or a journal
  entry.
* **Four looks:** *Classic* (a VGA-era DOS game launcher: pixel font, chunky bevels, striped title bars,
  a hint of GOG violet, soft grey text with orange headings; its font can be switched in Settings between
  Pixel Operator, Dogica, Geist Pixel and Roboto Slab), *Terminal CRT Amber* (soft amber with warm white
  and light red), *Matrix Terminal* (soft green with white and yellow) and *DOS Midnight* (MS-DOS text mode
  in the IBM VGA font, a shade darker). The terminal palettes are toned down so they are easy on the eyes.
  Themes change only the look; controls stay the same. The PDF viewer and the music player follow the
  chosen look.
* **English or Czech:** the whole interface is available in English and in Czech (Settings > Language,
  or right in the welcome window and the setup wizard). Messages from the app, the PDF viewer, the music
  player and the Windows file dialogs follow the choice; names such as Game Curator, GOG.com, Patch or
  Mod stay as they are, and your own data (titles, descriptions, notes) is never translated.
* A welcome window that explains the idea, with "Don't show this again".

## Screenshots

| | |
|---|---|
| ![Library grid](screenshots/09-library-grid.png) | ![Gaming Journal overview](screenshots/12a-journal-overview.png) |
| ![Screenshot gallery](screenshots/12f-gallery.png) | ![Archive Scanner](screenshots/08-scanner-results.png) |
| ![Welcome window](screenshots/00-welcome.png) | ![Cover crop editor](screenshots/04b-adjust-artwork.png) |
| ![Catalog view with icons of installed games](screenshots/10b-catalog-icons.png) | ![A file with a name in the catalog](screenshots/05b-file-renamed.png) |
| ![Journal offer after playing](screenshots/12c2-journal-offer.png) | ![Fonts of the Classic look](screenshots/12h-settings-fonts.png) |
| ![PDF manual in the built-in viewer](screenshots/05c-pdf-viewer.png) | ![Soundtrack in the music player](screenshots/05d-music-player.png) |
| ![Music player in DOS Midnight](screenshots/22b-dos-player.png) | ![PDF viewer in Matrix Terminal](screenshots/22c-matrix-viewer.png) |
| ![DOS Midnight theme](screenshots/21-dos-library.png) | ![Terminal CRT Amber theme](screenshots/22-amber-profile.png) |
| ![Matrix Terminal theme](screenshots/21-matrix-library.png) | ![Drag and drop onto Additional files](screenshots/12i-drag-over.png) |
| ![Filter by genre and tags](screenshots/09b-library-filter.png) | ![Catalog view](screenshots/10-library-catalog.png) |
| ![Game details from GOG.com to review](screenshots/08c-gog-review.png) | ![Details from GOG.com saved in the profile](screenshots/08d-gog-details-saved.png) |
| ![Czech descriptions from Databáze-her.cz to review](screenshots/08e-dbher-review-cs-descriptions.png) | |
| ![Archive Scanner with its guide, in Czech](screenshots/21b-scanner-help-cs.png) | ![Game form with the game folder help, in Czech](screenshots/21c-game-form-help-cs.png) |

## What to try first (testers)

If you test Game Curator on your own archive, these reports help most. Please open an issue in this repository.

* **Does it start?** Windows version, display scaling (100 / 125 / 150 %), and the WebView2 version shown
  in **About**.
* **Archive Scanner** on a real GOG collection: were titles and installers recognised correctly?
* **Game details from GOG.com, Databáze-her.cz and MobyGames:** were your games found, and was the right
  game picked? Which titles needed "Search again" (or a pasted Databáze-her.cz address)? Did your
  MobyGames API key work?
* **Installed games:** were your GOG installs detected? Does **Play** start them, including DOSBox and
  ScummVM games? When you quit a game (also after switching to another window), does the
  "Back from the game" offer appear on top within about a second?
* **Manuals and soundtracks:** open a real GOG manual (large scanned PDFs are the best test) and a
  soundtrack in MP3, FLAC or OGG. Does the first page appear quickly? Does the music play, and do the
  media keys work?
* **Drag & drop** from Explorer, especially with display scaling: do files land in the right panel?
* **Portability:** move the drive to another PC or change its letter. Does everything still open?
* **Feel:** scrolling a large library, the themes, and anything that feels slow, confusing or unnecessary.

## Project layout

```text
```text
crates/curator-core/   GUI-free Rust core: catalog, file index, scanner, journal, screenshots,
                       additional files, installed games, images (WebP), backups, migrations,
                       game details from GOG.com, Databáze-her.cz and MobyGames
src-tauri/             Tauri 2 shell: commands, Windows APIs (ShellExecuteExW, registry read),
                       gcimg:// protocol, drag & drop, watching a game started with Play, and the
                       PDF viewer, music player and journal-offer windows
ui/                    the interface: vanilla HTML/CSS/JS, themes, pixel web fonts; viewer.html,
                       player.html and offer.html for the extra windows (pdf.js in ui/vendor/pdfjs)
tests/e2e/             end-to-end tests (WebDriver)
tools/                 Windows build, packaging, icon and web-font generators, pdf.js copy script
docs/                  architecture, build, testing, known limitations
```
```
