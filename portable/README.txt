GAME CURATOR 0.1
Your games. Your archive. Your history.
==========================================================================

Osobní offline archiv DRM-free her: instalační soubory, manuály, soundtracky
a další obsah ke každé hře přehledně na jednom místě (např. offline
instalátory z GOG.com). Žádná instalace, žádné účty, žádný internet.

Archiv, ne backlog: program nesleduje, co je dohrané ani jak hry hodnotíte.
Pomáhá vyznat se v souborech a zapsat si, co o hře víte a co vás na ní zaujalo.

Ovládání je jen levé menu: Library, Gaming Journal, Archive Scanner, Settings
a dole About. Zápisky deníku a "What I know about this game" se píší
v profilu hry; stránka Gaming Journal je všechny shrnuje po hrách.
Když skončí hra spuštěná tlačítkem "Play", Game Curator do sekundy nabídne
zápis do deníku v malém okně, které je vždy navrchu, i když už jste
přepnuli do jiného programu (lze vypnout zaškrtnutím "Don't show this
again", zpět v Settings).
Herní čas se nikdy nepočítá.

Profil hry: dělicí čáru mezi levou částí (hra, poznámky, deník) a Archive
lze posunout myší; dvojklik ji vrátí na výchozí šířku.

Screenshots (pod deníkem v profilu hry): "Add screenshots..." přidá obrázky
ze hry (PNG, JPG, WebP); vidět jsou poslední 3, celá galerie po kliknutí.
Do archivu se ukládá optimalizovaná kopie, váš soubor zůstává, kde byl.
Screenshot lze připojit i k zápisku deníku (v editoru zápisku).

Soubory lze do Screenshots i Additional files také přetáhnout myší
z Průzkumníka Windows.

Additional files (pod Archive): opravy, komunitní patche, mody, bonusy.
Soubor se zkopíruje do Data\Additions (cestuje s diskem), nebo se na něj
jen odkáže tam, kde je.

Tužka (✎) u souboru v Archive i v Additional files mu dá název v katalogu.
Soubor na disku si nechá svůj název (ukáže se po najetí myší).

Manuály a soundtracky přímo v programu: Open u PDF otevře okno čtečky
(listování, zoom, "Fit width"; PageUp/PageDown, Ctrl +/-, Ctrl+kolečko).
Open u hudby (MP3, FLAC, OGG, WAV, M4A...) otevře malý přehrávač se
soundtrackem hry: přehrát/pauza, předchozí/další, posuvník, hlasitost,
opakování a seznam skladeb (mezerník = pauza, šipky = posun o 5 s).
Obě okna mají "Open in default app" (otevře soubor v programu Windows).
Settings > Manuals & music to celé vypne.

Obal hry: po výběru obrázku ho v rámečku posunete myší a přiblížíte
kolečkem nebo posuvníkem (Fit = celý obrázek). Později v profilu hry
tlačítkem Adjust na obalu.

Nainstalované hry: při startu Game Curator zjistí (jen čtením záznamů GOG
a seznamu programů Windows), které hry z katalogu jsou na tomto počítači
nainstalované. Ukazuje to ikonka zaškrtávátka. V profilu hry je pak
v rámečku "Installed on this PC" tlačítko "Play" (spustí hru po
potvrzení) a "Show location" (otevře složku instalace; cesta se jinak
nevypisuje, je vidět po najetí myší). Nenalezenou hru lze ukázat ručně
tlačítkem "Locate...". Instalace se pamatují zvlášť pro každý počítač.
V zobrazení Catalog mají nainstalované hry svou ikonu (z goggame-*.ico nebo
z .exe hry; soubory hry se jen čtou).


SPUŠTĚNÍ
--------
1. Zkopírujte celou složku GAME_CURATOR na externí disk (USB / SSD).
2. Spusťte GameCurator.exe.
3. Při prvním spuštění vás provede jednoduchý průvodce (lze přeskočit).

Požadavky: Windows 10 nebo 11 (64bit) s Microsoft Edge WebView2 Runtime.
Windows 11 a aktualizované Windows 10 ho už obsahují. Pokud chybí,
aplikace to oznámí srozumitelnou zprávou a nic nezmění. Řešení:
  a) nainstalujte "Evergreen WebView2 Runtime" od Microsoftu, nebo
  b) pro počítače bez internetu: stáhněte "Fixed Version" WebView2 Runtime
     a rozbalte ho do složky WebView2Runtime vedle GameCurator.exe
     (v ní musí ležet msedgewebview2.exe).


CO KDE LEŽÍ
-----------
GAME_CURATOR\
  GameCurator.exe        aplikace
  WebView2Loader.dll     součást aplikace (zavaděč WebView2 od Microsoftu)
  README.txt             tento soubor
  Licenses\              licence písem a knihovny pdf.js (čtečka PDF)
  .gamecurator-id        identifikátor archivu (nemazat, neupravovat)
  Data\
    library.db           databáze katalogu (SQLite)
    config.json          nastavení zobrazení
    Artwork\Covers\      obaly her (optimalizované WebP)
    Thumbnails\          náhledy pro mřížku knihovny
    Screenshots\         screenshoty her (optimalizované kopie)
    Additions\           dodatečné soubory zkopírované do archivu
    Journals\            čitelné HTML kopie zápisků herního deníku
    Backups\             zálohy databáze
    WebView\             mezipaměť zobrazovací komponenty (lze smazat)
  Games\                 doporučené místo pro herní složky

Hry mohou ležet i mimo složku Games, třeba v D:\GOG_Installers.
Pokud jsou na stejném disku jako archiv, ukládají se relativně
(..\GOG_Installers\Hra) a fungují i po změně písmene jednotky
nebo po přenesení celého disku na jiný počítač.


BEZPEČNOST VAŠICH DAT
---------------------
* Game Curator nikdy nemaže, nepřesouvá ani neupravuje herní soubory.
* Odebrání hry z katalogu smaže jen záznam v databázi, složka zůstává.
* Programy (.exe, .msi) se spouští jen tlačítkem Run a po potvrzení.
  Skener nikdy nic nespouští.
* Databáze se automaticky zálohuje (nejvýše jednou denně při startu)
  a ručně kdykoli v Settings. Obnova ze zálohy nejdřív zálohuje
  aktuální stav.
* Při odpojení disku se nic nemaže - záznamy se jen označí jako
  nedostupné (OFFLINE).
* Aplikace nezapisuje do registru ani do AppData a po zavření
  neběží na pozadí. Registr jen čte (seznam nainstalovaných her).


VZHLED
------
Settings > Appearance: Classic (spouštěč DOSových her z éry VGA, pixelové
písmo; písmo lze přepnout na Dogica, Geist Pixel nebo Roboto Slab), Terminal CRT Amber (tlumená jantarová, bílá, světle rudá), Matrix
Terminal (tlumená zelená, bílá, žlutá), DOS Midnight (MS-DOS s písmem IBM VGA,
o odstín tmavší). Mění se jen vzhled, ovládání zůstává stejné. Obaly her jsou
v terminálových vzhledech tónované, skutečné barvy ukáže najetí myší.
Pixelová písma (Classic, DOS Midnight) jsou nejostřejší při měřítku displeje
100 % nebo 200 %; při 125 % a 150 % jsou některé pixely písma o kousek širší.
Písma: Dogica a Dogica Pixel (c) 2020 Roberto Mocci, SIL Open Font License
1.1, použito beze změny; Geist Pixel (c) 2026 The Geist Project Authors,
SIL OFL 1.1; Roboto Slab, Apache License 2.0;
Pixel Operator od Jayvee Enaguas (CC0, volné dílo); PxPlus IBM VGA8
od VileR, The Ultimate Oldschool PC Font Pack (CC BY-SA 4.0,
https://creativecommons.org/licenses/by-sa/4.0/), použito beze změny.
PDF manuály zobrazuje PDF.js od Mozilly (Apache License 2.0) s jeho
standardními písmy a dekodéry obrázků; licence jsou ve složce Licenses.


KLÁVESOVÉ ZKRATKY
-----------------
Alt+1 .. Alt+5   Library / Gaming Journal / Archive Scanner / Settings / About
Ctrl+F           hledání v knihovně
Ctrl+N           přidat hru (v knihovně)
Ctrl+S           uložit (formulář hry, zápisek, "What I know about this game")
Esc              zavřít okno


ZNÁMÁ OMEZENÍ VERZE 0.1
-----------------------
Viz docs\KNOWN_LIMITATIONS.md ve zdrojovém kódu. Hlavně: jen Windows 10/11
(64bit), artwork je jen obal (pozadí a logo nejsou), rozhraní je
v angličtině, čtečka PDF neumí hledat ani kopírovat text (na to slouží
"Open in default app").
