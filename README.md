<p align="center">
  <img src="src-tauri/icons/icon.png" width="96" alt="Game Curator icon: three big-box games on a shelf, one of them open">
</p>

<h1 align="center">Game Curator</h1>

<p align="center">
  <b>Your games. Your archive. Your history.</b><br>
  A portable, offline archive for your DRM-free game collection, built for Windows.
</p>

<p align="center">
  <img alt="Windows 10/11" src="https://img.shields.io/badge/platform-Windows%2010%20%2F%2011-5c3d99">
  <img alt="Portable" src="https://img.shields.io/badge/portable-no%20install-5c3d99">
  <img alt="Offline" src="https://img.shields.io/badge/network-only%20game%20details%2C%20on%20request-5c3d99">
  <img alt="English / Czech" src="https://img.shields.io/badge/language-English%20%7C%20%C4%8Ce%C5%A1tina-5c3d99">
  <img alt="Status" src="https://img.shields.io/badge/status-early%20preview%20(0.2)-b58a2a">
</p>

![Game profile: artwork, installed game with Play, notes, journal, archive files and additional files](docs/screenshots/12d-profile-installed.png)

## The idea

DRM-free games come with a lot: offline installers, patches, manuals, soundtracks, artbooks. Over the
years the drive fills up and you no longer know what you have or where it is. Game Curator puts it in
order, and gives every game a place for what you know about it.

**It is an archive, not a backlog.** There is no "completed", no ratings, no play time: numbers like
these quietly push you to *finish* games. Game Curator is about *knowing* them. Look at your collection
like a curator would: notice the details, find out how a game was made, and write down what you
discovered. Slowly, and with attention.

## What it does

* **Library:** a cover grid or a compact catalog, search that ignores diacritics, and a filter by genre and tags.
* **Game profile:** details and artwork, free notes (*What I know about this game*), a dated **Gaming
  Journal** with screenshots, and every file in the game's folder sorted into installers, manuals, music
  and extras.
* **Archive Scanner:** finds the installer folders on your drive, with a short built-in guide, and
  imports them without duplicates.
* **Game details on request** from **GOG.com**, **Databáze-her.cz** (Czech descriptions) or **MobyGames**
  (English descriptions). You review everything before it is saved, and your own text is kept unless you
  tick it.
* **Installed games:** detected (read-only) and started with **▶ Play**. When the game ends, Game Curator
  offers to write down your impressions.
* **Manuals and soundtracks** open in a built-in PDF reader and music player.
* **Four retro looks** (VGA-era launcher, amber CRT, green terminal, DOS), and the interface in **English
  or Czech**.

The full tour with screenshots is in [docs/FEATURES.md](docs/FEATURES.md).

## Principles

* **Portable:** it runs from an external drive. Everything lives next to the `.exe`, with paths stored
  relative to the archive. No installer, nothing in AppData, nothing written to the registry.
* **Offline:** no accounts, no cloud, no telemetry. Game Curator only goes online when you ask for game
  details, and then contacts only the source you chose.
* **Safe:** game files are never deleted, moved or changed, and programs start only after you confirm. A
  disconnected drive shows its games as OFFLINE; nothing is removed.
* **Yours:** a plain SQLite database, journal entries as readable HTML, and backups in a folder you can see.

## Get started

1. Download `GameCurator-0.2.0-windows-portable.zip` from [Releases](../../releases).
2. Extract it to your games drive (an external drive is ideal).
3. Run `GAME_CURATOR\GameCurator.exe`. Add a game, or let the Archive Scanner find your installers.

You need Windows 10 or 11 (64-bit) with the Microsoft Edge WebView2 Runtime, which Windows 11 and updated
Windows 10 already have.

## Status: early preview, testers wanted

Version 0.2 is feature-complete, but **it has not run on a real Windows machine yet**. The `.exe` is
cross-compiled and checked by 68 automated tests and 250 end-to-end checks that drive the real app on Linux.

Try it on your own archive and [open an issue](../../issues): Does it start? Does the scanner recognise your
collection? Are your games found online? What feels slow, confusing or unnecessary? The list of what helps
most is in [docs/FEATURES.md](docs/FEATURES.md#what-to-try-first-testers).

## Build from source

```powershell
cargo build --release -p game-curator     # -> target\release\GameCurator.exe
cargo test  -p curator-core               # core tests, run on any OS
```

You need Rust (MSVC) and the Visual Studio Build Tools; no Node.js. The UI is plain HTML/CSS/JS embedded in
the `.exe`. More in [docs/BUILD.md](docs/BUILD.md).

## More

* [Česky](docs/README.cs.md) · [Feature tour](docs/FEATURES.md) · [Credits and third-party licences](docs/CREDITS.md)
* In Czech: [architecture](docs/ARCHITECTURE.md), [build](docs/BUILD.md), [test results](docs/TESTING.md),
  [known limitations](docs/KNOWN_LIMITATIONS.md), [database schema](docs/schema.sql)
* **License:** to be decided. Until a license is added, all rights are reserved by the author.

---

Game Curator is an independent hobby project, not affiliated with GOG.com, CD PROJEKT, Databáze-her.cz or
MobyGames. Game details fetched from these sources belong to their owners and stay in your own catalog;
descriptions from Databáze-her.cz are used under CC BY-NC-ND 3.0 CZ with their source written under them.
