GAME CURATOR 0.1
Your games. Your archive. Your history.
==========================================================================
 
A personal offline archive for your DRM-free games: installers, manuals,
soundtracks and other content for every game, neatly in one place (e.g.
offline installers from GOG.com). No installation, no accounts, no internet.
 
An archive, not a backlog: the program does not track which games you have
finished or how you rate them. It helps you find your way around your files
and write down what you know about a game and what caught your attention.
 
Everything is controlled from the menu on the left: Library, Gaming Journal,
Archive Scanner, Settings and, at the bottom, About. Journal entries and
"What I know about this game" are written in the game's profile; the Gaming
Journal page brings them all together, game by game.
 
Game profile: the divider between the left part (game, notes, journal) and
Archive can be moved with the mouse; a double-click resets it to the default
width.
 
Screenshots (below the journal in the game's profile): "Add screenshots..."
adds pictures from the game (PNG, JPG, WebP); the latest 3 are shown and the
whole gallery opens with a click. An optimized copy is stored in the archive;
your file stays where it was. A screenshot can also be attached to a journal
entry (in the entry editor).
 
Files can also be dragged into Screenshots and Additional files straight from
Windows Explorer.
 
Additional files (below Archive): fixes, community patches, mods, bonus
material. A file is either copied into Data\Additions (it travels with the
drive) or just linked where it is.
 
Game cover: after choosing a picture, move it within the frame with the mouse
and zoom with the mouse wheel or the slider (Fit = the whole picture). Later,
use the Adjust button on the cover in the game's profile.
 
Installed games: at startup Game Curator finds out (only by reading the GOG
records and the Windows list of programs) which games from the catalog are
installed on this computer. A checkbox icon shows it. In the game's profile,
the "Installed on this PC" box then offers "Play" (starts the game after a
confirmation) and "Show location" (opens the installation folder). A game
that was not found can be pointed to manually with "Locate...".
Installations are remembered separately for each computer.
 
 
GETTING STARTED
---------------
1. Copy the whole GAME_CURATOR folder to an external drive (USB / SSD).
2. Run GameCurator.exe.
3. On the first start, a short wizard guides you through the setup (it can
   be skipped).
 
Requirements: Windows 10 or 11 (64-bit) with the Microsoft Edge WebView2
Runtime. Windows 11 and updated Windows 10 already include it. If it is
missing, the app tells you so in a clear message and changes nothing.
Solutions:
  a) install the "Evergreen WebView2 Runtime" from Microsoft, or
  b) for computers without internet: download the "Fixed Version" WebView2
     Runtime and extract it into a WebView2Runtime folder next to
     GameCurator.exe (msedgewebview2.exe must be inside it).
 
 
WHAT LIVES WHERE
----------------
GAME_CURATOR\
  GameCurator.exe        the application
  WebView2Loader.dll     part of the application (Microsoft's WebView2 loader)
  README.txt             this file
  .gamecurator-id        archive identifier (do not delete or edit)
  Data\
    library.db           catalog database (SQLite)
    config.json          display settings
    Artwork\Covers\      game covers (optimized WebP)
    Thumbnails\          previews for the library grid
    Screenshots\         game screenshots (optimized copies)
    Additions\           additional files copied into the archive
    Journals\            readable HTML copies of gaming journal entries
    Backups\             database backups
    WebView\             cache of the display component (can be deleted)
  Games\                 suggested place for game folders
 
Games can also live outside the Games folder, e.g. in D:\GOG_Installers.
If they are on the same drive as the archive, they are stored relative to it
(..\GOG_Installers\Game) and keep working after the drive letter changes or
when the whole drive is moved to another computer.
 
 
THE SAFETY OF YOUR DATA
-----------------------
* Game Curator never deletes, moves or modifies game files.
* Removing a game from the catalog only deletes its database record; the
  folder stays.
* Programs (.exe, .msi) are started only with the Run button and after
  a confirmation. The scanner never starts anything.
* The database is backed up automatically (at most once a day, at startup)
  and manually at any time in Settings. Restoring a backup first backs up
  the current state.
* When a drive is disconnected, nothing is deleted - the records are only
  marked as unavailable (OFFLINE).
* The app does not write to the registry or to AppData and does not keep
  running in the background after it is closed. It only reads the registry
  (the list of installed games).
 
 
LOOKS
-----
Settings > Appearance: Classic (dark, with a hint of violet), Terminal CRT
Amber, Matrix Terminal, DOS Midnight (MS-DOS, a shade darker). Only the look
changes; the controls stay the same. In the terminal themes, game covers are
tinted; hover over a cover to see its real colors.
 
 
KEYBOARD SHORTCUTS
------------------
Alt+1 .. Alt+5   Library / Gaming Journal / Archive Scanner / Settings / About
Ctrl+F           search the library
Ctrl+N           add a game (in the library)
Ctrl+S           save (game form, journal entry, "What I know about this game")
Esc              close a window
 
 
KNOWN LIMITATIONS OF VERSION 0.1
--------------------------------
See docs\KNOWN_LIMITATIONS.md in the source code. Mainly: Windows 10/11
(64-bit) only, the only artwork is the cover (no background or logo), and
the interface is available in English only.
 
