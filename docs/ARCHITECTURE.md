# Game Curator 0.2 – architektura a struktura projektu

Cílová platforma: **jen Windows 10/11 x64** s WebView2. Kód pro jiné systémy
neexistuje; jádro je čistý Rust (testovatelný kdekoli) a linuxový build
aplikace slouží výhradně jako testovací harness (`--features e2e`).

## Přehled vrstev

```
┌───────────────────────────────────────────────────────────────┐
│ ui/  (HTML + CSS + vanilla JavaScript, ES moduly, bez frameworku)│
│   app.js  router, stavový řádek, klávesové zkratky               │
│   css/retro.css (základ) + classic.css + themes.css (Amber,     │
│   Matrix, DOS); fonts/ pixelové písmo Pixel Operator (CC0)      │
│   library / profile / journal / scanner / settings / about /    │
│   wizard; levé menu s ikonami je jediná navigace (bez top baru) │
│   api.js  jediné místo, které volá Tauri IPC                    │
│   viewer.html + viewer.js  čtečka PDF (pdf.js, ui/vendor/pdfjs) │
│   player.html + player.js  hudební přehrávač soundtracku        │
└──────────────────────────────┬────────────────────────────────┘
                               │ invoke() + události (scan-progress…)
┌──────────────────────────────▼────────────────────────────────┐
│ src-tauri/  (tenká aplikační vrstva, Tauri 2)                   │
│   commands.rs  ~35 příkazů, jen převod argumentů a chyb          │
│   system.rs    Windows API: ShellExecuteW (Open/Run/Play),        │
│                MessageBoxW, GetDriveTypeW, kontrola WebView2,    │
│                čtení registru (GOG, Uninstall) – jen čtení       │
│   protocol.rs  gcimg:// – servíruje obaly jen z Data/Artwork,    │
│                Data/Thumbnails                                   │
│   viewer.rs    okna čtečky PDF a přehrávače, tokeny souborů,     │
│                čtení po částech (media_read)                     │
│   details.rs   údaje o hrách (GOG/DH/Moby): vlákno, revize, uložení│
│   state.rs     sdílený stav (Mutex<Database>, config, sken)      │
│   lib.rs       start: archiv → WebView2 → DB → úklid → okno      │
└──────────────────────────────┬────────────────────────────────┘
                               │ čisté Rust API
┌──────────────────────────────▼────────────────────────────────┐
│ crates/curator-core/  (bez GUI závislostí, plně testovatelné)   │
│   archive.rs  struktura disku, .gamecurator-id, otevření archivu │
│   db.rs       připojení, PRAGMA, migrace, settings              │
│   paths.rs    přenosné (relativní) cesty, kontrola „uvnitř složky“│
│   games.rs    katalog: CRUD, štítky, dotazy knihovny, dostupnost │
│   files.rs    obsah herní složky, kategorie, hlavní instalátor   │
│   journal.rs  herní deník + HTML kopie v Data/Journals           │
│   installs.rs nainstalované hry: párování, záznamy pro každé PC   │
│   icons.rs    ikony nainstalovaných her (jen čtení .ico / .exe)  │
│   screenshots.rs screenshoty (galerie, vazba na zápisek deníku)  │
│   additions.rs   dodatečné soubory: kopie do archivu / odkaz     │
│   media.rs    PDF/hudba: typy, playlist hry, čtení po částech     │
│   scanner.rs  hledání kandidátů, zrušení, import bez duplicit    │
│   lookup.rs   společné pro údaje o hrách: povolené adresy zdroje,│
│               Fetcher (HTTPS, pauzy), párování názvů, HTML → text,│
│               návrh změn, uložení vybraného, uvedení zdroje      │
│   gog.rs      GOG.com: katalog + produktová API                  │
│   dbher.rs    Databáze-her.cz: adresa stránky z názvu, čtení HTML │
│   moby.rs     MobyGames API (klíč uživatele, max. 1 dotaz/s)     │
│   images.rs   JPG/PNG/WebP → WebP obal + náhled 360×480 z výřezu │
│   backup.rs   VACUUM INTO zálohy, ověření, rotace, obnova        │
│   config.rs   Data/config.json                                   │
│   schema.sql  databázové schéma                                  │
└───────────────────────────────────────────────────────────────┘
```

Frontend nikdy nesahá na souborový systém. JavaScript smí jen volat
příkazy aplikace (capability `core:default`); dialogy i otevírání souborů
dělá Rust, který každou cestu ověří.

## Rozhraní

* **Jediné menu je levý sloupec**: logo (otevřená krabice), pod ním Library,
  Gaming Journal, Archive Scanner, Settings a dole About. Ikony jsou inline
  SVG s `stroke: currentColor`, takže je přebarví každý vzhled. Alt+1–5.
* **Library** má hledání podle názvu, řazení a **filtr podle žánru
  a štítků** (`LibraryQuery { search, sort, genres, tags }`; tlačítko
  s trychtýřem vedle Sort). Pole žánru obsahuje víc žánrů oddělených
  čárkou (`games::split_genres`); filtr i nabídka (`library_facets`) je
  berou jednotlivě, bez ohledu na velikost písmen, diakritiku a mezery.
  V rámci žánrů i štítků stačí jeden zaškrtnutý („nebo“), mezi žánry
  a štítky musí platit obojí („a“). Výběr se drží po dobu běhu aplikace
  (`state.query`), aktivní filtry ukazuje lišta nad hrami (× odebere jeden,
  „Clear filters“ všechny); hodnoty, které už žádná hra nemá, se zahodí.
  Filtry podle stavu hraní nejsou; staré klíče ze 0.1 se ignorují.
  Stavový řádek ukazuje „N games · M offline“ (`facets.js`). Pole Genre
  ve formuláři nenabízí hodnoty z jiných her (žádný `datalist`,
  `autocomplete="off"`).
* **Zápisky** vznikají jen v profilu hry. Stránka Gaming Journal načte
  všechny zápisky (`list_journal`, nejnovější první) a seskupí je po hrách;
  pořadí skupin určuje nejnovější zápisek. `navigate('journal', { gameId })`
  z profilu odroluje na skupinu dané hry.
* **Profil hry**: mřížka se třemi sloupci – levý (hra, poznámky, deník,
  screenshoty), dělicí čára (`.pf-split`, role `separator`) a pravý
  (Archive, pod ním Additional files). Šířka levého sloupce
  je proměnná CSS `--split` v procentech; tažení, šipky a dvojklik ji mění,
  ukládá se do `config.json` (`profileSplit`, 25–75 %, výchozí 42,5 %). Oba
  sloupce si drží minimální šířku (320 / 380 px). Pod 1180 px se panely
  skládají pod sebe a čára zmizí.
* **About** je statický text s hlavní myšlenkou + údaje z `app_info`.

## Jazyk rozhraní (angličtina / čeština)

* **Nastavení → Language / Jazyk** (a přepínač „English | Čeština“ přímo
  v uvítacím okně a v průvodci). Volba se ukládá do `config.json`
  (`language`: `"en"` výchozí | `"cs"`; neznámá hodnota → `"en"`).
* **Překládá UI**, Rust píše anglicky. `ui/js/i18n.js`: anglický text ve
  zdrojáku je klíč, `t('…', { params })` vrátí překlad z `ui/js/lang/cs.js`
  (chybějící klíč = angličtina, nic se nerozbije). `tn(n, one, other)` řeší
  české tři tvary množného čísla (1 / 2–4 / 0 a 5+); klíčem je anglický
  tvar „other“ a hodnotou pole `[one, few, many]`. `mark('…')` označí texty
  v tabulkách (kategorie souborů, druhy dodatečných souborů, popisy vzhledů),
  které se přeloží až při zobrazení.
* **Zprávy z Rustu** (chyby, poznámky při startu, typ disku) překládá `tm()`
  v jediném místě – `api.js call()` – podle `CS_MESSAGES`: přesná shoda nebo
  vzor s `{0}`, `{1}` (cesty, čísla, názvy zůstanou, vnořená zpráva se
  přeloží znovu). Počty ve zprávách z Rustu jsou psané s dvojtečkou („Odstraněné
  nepoužívané soubory obalů: 3.“), takže nepotřebují tvary množného čísla.
* **Statické HTML** (levý sloupec, stavový řádek, okna PDF, přehrávače
  a nabídky zápisu) nese `data-i18n`, `data-i18n-title`, `data-i18n-aria`,
  `data-i18n-doc`; `applyStatic()` je přeloží při načtení i při přepnutí.
* **Přepnutí bez restartu**: `store.changeLanguage()` uloží volbu, pošle
  událost `gc-lang` (app.js přejmenuje levý sloupec a stavový řádek)
  a znovu vykreslí aktuální stránku. Ostatní okna (PDF, hudba, nabídka
  zápisu) čtou jazyk synchronně z `localStorage` (`gc-lang`, mezipaměť
  config.json – stejně jako vzhled) a přepnou se přes událost `storage`.
  Rozpracované texty (stav načítání, zpráva přehrávače) se drží jako funkce,
  takže se přeloží i dodatečně.
* **Rust** (`src-tauri/src/lang.rs`) přeloží jen to, co kreslí Windows: titulky
  souborových dialogů, titulky oken („Hudba“, „Zpátky ze hry“) a chybová okna
  při startu (WebView2 chybí, archiv nejde otevřít – jazyk se čte z
  `Data/config.json` ještě před otevřením archivu).
* **Co zůstává anglicky**: názvy (Game Curator, GOG.com, názvy vzhledů a písem),
  herní výrazy, které čeští hráči používají (Patch, Fix, Mod, DLC, screenshot,
  soundtrack), razítko OFFLINE. Data uživatele (názvy, popisy, poznámky,
  žánry, tagy) se nepřekládají nikdy. Oslovení je tykání.
* **Kontrola úplnosti**: `tools/check_i18n.py` posbírá všechny klíče
  (`t`, `tn`, `mark`, `data-i18n*`) i uživatelské zprávy z Rustu a ověří, že
  mají český překlad se stejnými zástupnými symboly a tři tvary u množného
  čísla. Spouští ho i E2E test.

## Výřez obalu

Obal (`Data/Artwork/Covers`, max. 1200×1600) si drží celý obrázek. Náhled
(`Data/Thumbnails`) je přesně to, co ukazuje rámeček karty: 360×480 (3:4),
vykreslený z výřezu, který uživatel zvolil v editoru (`cropper.js`: tažení,
kolečko, posuvník od „Fit“ po 5× „Fill“). Výřez se ukládá jako
`games.cover_crop` = JSON se zlomky šířky a výšky obalu, takže ho jde kdykoli
změnit bez nového nahrání. Může přesahovat obrázek (oddálení); volná plocha
se vyplní tmavým odstínem průměrné barvy obrázku. Každé nové vykreslení
zapíše nový soubor (žádná stará verze v cache WebView) a starý smaže.

## Uvítací okno

* `welcome.js`: při startu (před průvodcem prvního spuštění), dokud není
  zaškrtnuto „Don't show this again“ → `config.json` `showWelcome: false`.
  Zapnout znovu jde v Settings → Appearance, otevřít kdykoli z About.

## Drag & drop

* Tauri zachytí přetažené soubory (`WindowEvent::DragDrop`); `dragdrop.rs`
  každému souboru přidělí jednorázový token (stejný mechanismus jako dialog
  souboru) a do UI pošle jen jména, velikosti a pozici ukazatele
  (`gc-drag`, `gc-drop`, v CSS pixelech). Cesty zůstávají v Rustu.
* `dropzone.js`: prvek s atributem `data-dropzone` pod ukazatelem dostane
  událost `gc-files`; během tažení se cílové plochy zvýrazní. Plochy:
  Screenshots (profil), Additional files (profil), galerie, screenshoty
  v editoru zápisku. Složky se vynechají.
* E2E sestavení má příkaz `e2e_drop`, který projde stejnou cestou jako
  skutečné puštění souborů.

## Screenshoty

* `screenshots.rs` + `images::process_screenshot`: obrázek (PNG/JPG/WebP,
  max. 50 MB) se uloží jako WebP q90 do `Data/Screenshots/game-<id>/`
  (max. 3840 px – 4K zůstane 4K) a náhled 16:9 480×270 do `Data/Thumbnails`.
  Originál se jen čte; jako „pořízeno“ se bere čas změny původního souboru.
* Screenshot patří hře a volitelně jednomu zápisku stejné hry
  (`journal_entry_id`, při smazání zápisku `SET NULL` – screenshot zůstane
  v galerii). Když se zápisek přesune k jiné hře, screenshoty jdou s ním.
* HTML kopie zápisku v `Data/Journals` obsahuje screenshoty jako relativní
  odkazy (`../../Screenshots/...`), takže je čitelná i bez aplikace.
* Úklid při startu maže jen nedokončené `.tmp` a osiřelé náhledy; samotné
  obrázky nikdy (třeba po obnovení starší zálohy databáze zůstanou na disku).
* Protokol `gcimg` nově vydává i `Data/Screenshots/**`.
* V editoru zápisku se nový obrázek nejdřív přidá do galerie hry a k zápisku
  se připojí až při uložení; při zrušení zůstane v galerii.

## Dodatečné soubory

* `additions.rs`: druh (fix, patch, mod, bonus, other), poznámka, velikost.
  **Kopie** do `Data/Additions/game-<id>/` (volné jméno „X (2).zip“,
  kopíruje se do `.part` a přejmenuje až po kontrole velikosti; kopírování
  běží mimo zámek databáze), nebo **odkaz** – uvnitř archivu relativní cesta
  (cestuje s diskem), jinde absolutní (ukáže se „this PC“).
* Cestu k souboru zná jen Rust: dialog vrátí UI jednorázový token,
  „Add“ pošle token, ne cestu.
* Open neotevře programy ani skripty (stejná pravidla jako Archive); `.exe`
  a `.msi` jde spustit tlačítkem Run po potvrzení.
* Odkazovaný soubor se nikdy nemaže. Kopii smaže jen uživatel (zaškrtávátko
  při „Take off the list“). Při odebrání hry z katalogu zůstávají screenshoty
  i kopie ve `Data/` ve výchozím stavu zachované; smažou se jen na výslovnou
  volbu v potvrzovacím okně.

## Názvy souborů v katalogu

* Tužka ✎ u souboru v Archive (vedle Open/Run) a u dodatečného souboru dává
  souboru **název v katalogu** (`files.display_name`, `additions.display_name`).
  Soubor na disku se nikdy nepřejmenuje – platí pravidlo „hra se nemění“;
  skutečný název je v nápovědě (tooltip) a v okně úprav.
* `text::display_name`: ořízne a sloučí mezery, max. 120 znaků, žádné řídicí
  znaky; prázdný název nebo název shodný se souborem znamená „zobraz název
  souboru“ (`NULL`). Název přežije každé nové načtení složky (synchronizace
  `files` používá `ON CONFLICT … DO UPDATE` jen pro údaje ze složky).

## Čtečka PDF a hudební přehrávač

* **Open** u PDF (soubor v Archive i dodatečný soubor) otevře okno čtečky,
  u hudby (mp3, ogg/oga/opus, flac, wav, m4a, aac) okno přehrávače se
  soundtrackem hry. Rozhoduje Rust (`commands::open_file` / `open_addition`
  → `viewer::open_archive_file` / `open_addition_file`); vypnout to lze v
  Settings > Manuals & music (`config.builtinViewer`), pak jde vše do
  výchozích programů Windows jako dřív. Obě okna mají „Open in default app“.
* Okna jsou stránky aplikace (`viewer.html`, `player.html`) vytvořené
  z kódu (`WebviewWindowBuilder`) se **stejným profilem WebView2**
  (`Data/WebView`, stejné přepínače) jako hlavní okno – nic nejde do
  AppData. Čtečka má label `pdf-<hash cesty>` (stejný soubor podruhé =
  stejné okno do popředí), přehrávač je jeden (`player`; nový soundtrack ho
  přepne událostí `gc-player-load`). Zavřením hlavního okna se zavřou i ona.
* Stránka nikdy nedostane cestu: Rust zaregistruje soubor vyřešený
  z katalogu pod **token** (`viewer::Viewers`) a stránka čte bajty přes
  `media_read(token, offset, length)` (max. 16 MB naráz, vrací surový
  ArrayBuffer). Čtečka zjistí svůj dokument podle labelu okna
  (`viewer_document`), přehrávač si vyžádá playlist (`player_state`).
  Tokeny zaniknou se zavřením okna. Vše jen čte.
* **pdf.js** (pdfjs-dist 6.5.438, „legacy“ build s polyfilly, Apache 2.0)
  je přibalený v `ui/vendor/pdfjs/` (`tools/vendor_pdfjs.sh`): knihovna,
  worker, standardní písma, dekodéry JPEG 2000 / JBIG2 / ICC (WASM +
  JS záloha). Bez sandboxu skriptů – JavaScript v PDF se nikdy nespouští,
  `isEvalSupported: false`, žádné XFA ani formuláře. Dokument se načítá
  po rozsazích (`PDFDataRangeTransport`, bloky 512 kB), takže první strana
  velkého skenovaného manuálu je vidět hned. Stránky se kreslí na canvas
  podle toho, co je vidět (+ sousední), drží se nejvýš 12 vykreslených,
  ostrost podle `devicePixelRatio`, limit 16 Mpx na canvas. Zoom: na šířku,
  celá strana, 25–400 %, Ctrl+kolečko, Ctrl ±/0; PageUp/PageDown, Ctrl+G.
* **Přehrávač** načte stopu přes `media_read` do paměti (Blob) a přehrává
  z ní, takže přetáčení je okamžité i na pomalém USB disku. Playlist =
  hudební soubory ze složky hry (přirozené řazení, „Track 2“ před
  „Track 10“; chybějící soubory a záznamy mimo složku vynechá) + hudební
  dodatečné soubory; názvy z katalogu. Ovládání: play/pause, předchozí
  (do 3 s od začátku stopy předchozí, jinak na začátek), další, posuvník,
  hlasitost + ztlumení, opakování (vypnuto / vše / jedna stopa), mezerník
  a šipky, Media Session (multimediální klávesy Windows). Hlasitost a
  opakování si pamatuje v localStorage profilu WebView. WebView2 běží
  s `--autoplay-policy=no-user-gesture-required`, aby stopa, kterou uživatel
  otevřel v hlavním okně, začala hrát bez dalšího kliknutí v přehrávači.
* CSP: `script-src 'self' 'wasm-unsafe-eval'` (WASM dekodéry pdf.js),
  `connect-src 'self'` (písma a WASM z aplikace), `media-src 'self' blob:`,
  `img-src … blob:`, `worker-src 'self'`; capability platí pro okna
  `main`, `player` a `pdf-*`.

## Nainstalované hry

* Aplikace při startu (a na tlačítko v Settings) přečte **jen pro čtení**
  `HKLM\SOFTWARE\GOG.com\Games` a seznamy `...\CurrentVersion\Uninstall`
  (HKLM 64/32 bit, HKCU). Do registru nikdy nezapisuje. Trvá to milisekundy,
  nic neběží na pozadí.
* `installs.rs` páruje přísně: stejný název po normalizaci (diakritika,
  ™, apostrofy, „&“ = „and“, úvodní „The“), nebo stejný název jako instalační
  soubor (`setup_system_shock_2_…` → „system shock 2“); edice za pomlčkou se
  ignoruje („Disco Elysium - The Final Cut“), dvojtečka ne (Fallout: New Vegas
  ≠ Fallout). Nejednoznačná shoda se zahodí – raději „nenalezeno“ než omyl.
* Program ke spuštění: primární úloha z `goggame-*.info` ve složce instalace,
  jinak `exe` z registru / `DisplayIcon`. Musí ležet uvnitř složky instalace
  (ověřeno i po rozbalení odkazů), `unins*.exe` se nebere.
* Záznamy jsou v tabulce `installations` s klíčem (hra, název počítače):
  archiv cestuje mezi počítači, instalace ne. Automatické záznamy se při
  každém startu obnoví, ruční („Locate…“) mění jen uživatel.
* UI posílá jen id hry; cestu k programu bere Rust z databáze. Play vyžaduje
  potvrzení (dialog + `confirmed: true` na úrovni příkazu).
* **Po dohrání** (`src-tauri/src/session.rs`): Play spouští hru přes
  `ShellExecuteExW` s `SEE_MASK_NOCLOSEPROCESS`, takže má handle procesu.
  Jedno vlákno na spuštěnou hru počká na tento proces a pak – protože mnoho
  her startuje přes launcher, který hned skončí – 4× za sekundu zjišťuje,
  zda běží nějaký program ze složky instalace (Toolhelp32 snímek; plnou
  cestu programu přes `QueryFullProcessImageNameW` zjišťuje jen jednou pro
  každý nový proces; jen čtení; nikdy kořen disku ani sdílené systémové
  složky). Když 0,75 s neběží nic ze složky, hra skončila (krátká mezera
  při restartu hry, např. po změně rozlišení, se za konec nepovažuje).
  Jen když spuštěný program skončil do 20 s a nic dalšího ze složky
  neběží (launcher předává hru se zpožděním), čeká se na hru až 8 s.
  Pak událost `gc-game-stopped` (Play znovu dostupné, během hry
  „● Running“) a, pokud hra běžela aspoň 15 s, **nabídka zápisu do deníku
  zhruba do sekundy od konce hry** (`src-tauri/src/offer.rs`): malé okno
  `journal-offer` (`offer.html`) uprostřed obrazovky, **vždy navrchu** –
  je vidět, i když už je vpředu jiný program –, ale bez převzetí
  klávesnice (`focused(false)`), s blikáním na liště. „Write entry“ okno
  zavře, přenese hlavní okno dopředu a otevře nový zápisek v profilu hry
  (událost `gc-journal-write`); „Not now“ / ✕ okno zavře; „Don't show
  this again“ → `config.journalPrompt = false` (hlavní okno to hned ví:
  `gc-config-changed`), zpět v Settings > Gaming Journal. Skončí-li víc
  her, nabídnou se postupně. Kdyby se okno nepodařilo otevřít, nabídne se
  dialog v hlavním okně. Herní čas se nikam neukládá ani
  neukazuje. Sledují se jen hry spuštěné z Game Curatoru a jen dokud běží
  Game Curator – po jeho zavření nic dál neběží. Na Linuxu (testy) se
  programy čtou z `/proc/*/exe`.
* Profil cestu k instalaci nevypisuje (méně textu na obrazovce): je
  v nápovědě rámečku a „Show location“ otevře složku.
* **Ikony** (`icons.rs`) pro zobrazení Catalog: při detekci (start, tlačítko
  v Settings) a po „Locate…“ se pro nainstalované hry bez ikony přečte
  `goggame-*.ico` ze složky instalace, jinak ikona ze spouštěného `.exe`.
  Program se nespouští ani nemění: parser čte jen hlavičky PE a sekci
  `.rsrc` (RT_GROUP_ICON → RT_ICON, první skupina = ikona, kterou ukazuje
  Průzkumník), se striktní kontrolou mezí (max. 64 kB na hlavičky, 4 MB na
  obrázek). Největší obrázek se zmenší (nebo malé 16/32 px zvětší po celých
  pixelech) na 48 × 48 a uloží jako bezeztrátové WebP do
  `Data/Thumbnails/icon-*.webp` (cesta v `games.icon_path`, zobrazuje se ve
  24 px). Nečitelná ikona = hra prostě ikonu nemá. Nepoužité ikony uklidí
  `cleanup_orphans`, odebrání hry smaže i její ikonu.

## Struktura repozitáře

```
game-curator/
├── Cargo.toml                 workspace (core + aplikace), release profil (LTO, strip)
├── .cargo/config.toml         linker pro cross-build na Windows (MinGW)
├── crates/curator-core/
│   ├── src/*.rs               jádro (viz výše)
│   ├── src/schema.sql         SQLite schéma (verze 5)
│   └── tests/acceptance.rs    integrační testy = akceptační testy 1–8 na úrovni jádra
├── src-tauri/
│   ├── Cargo.toml             binárka GameCurator, featura `e2e` jen pro testy
│   ├── tauri.conf.json        CSP, žádná okna z configu (okno se tvoří v kódu)
│   ├── tauri.linux.conf.json  jen CSP pro testovací harness (WebKitGTK)
│   ├── capabilities/          minimální oprávnění JS
│   ├── icons/                 ikony (generuje tools/make_icons.py)
│   └── src/                   lib.rs, commands.rs, system.rs, protocol.rs, state.rs,
│                              dragdrop.rs, session.rs (konec hry spuštěné přes Play),
│                              viewer.rs (okna čtečky PDF a přehrávače), offer.rs (nabídka
│                              zápisu po hraní), details.rs (údaje o hrách z internetu), lang.rs (jazyk
│                              dialogů a titulků oken Windows)
├── ui/                        index.html, viewer.html, player.html, offer.html,
│                              css/retro.css + classic.css + themes.css + media.css,
│                              js/*.js (i18n.js = jazyk rozhraní), js/lang/cs.js (čeština), img/, vendor/pdfjs/ (pdf.js – Apache 2.0), fonts/ (Pixel Operator – CC0, PxPlus IBM
│                              VGA8 – CC BY-SA 4.0, Dogica – OFL, Geist Pixel – OFL,
│                              Roboto Slab – Apache 2.0, GC VGA / Pixel Symbols – CC0)
├── portable/README.txt        návod, který se přikládá k GameCurator.exe
├── tests/e2e/run_e2e.py       end-to-end testy přes WebDriver (tauri-driver)
├── tools/                     build-windows.sh, package.sh, make_icons.py, make_matrix_rain.py,
│                              make_fonts.py + fonts-src/ (originální TTF a licence písma),
│                              vendor_pdfjs.sh (kopie pdf.js do ui/vendor/pdfjs),
│                              check_i18n.py (úplnost českého překladu)
└── docs/                      tato dokumentace
```

## Přenositelnost (klíčový požadavek)

* **Kořen archivu** = složka, ve které leží `GameCurator.exe`
  (`std::env::current_exe()`). Proměnná `GAME_CURATOR_ROOT` ho může přepsat
  (vývoj a testy).
* **Všechna data** jsou v `Data/` vedle exe: databáze, config, artwork,
  náhledy, deníky, zálohy a také **profil WebView2** (`Data/WebView`).
  Bez toho by WebView2 zapisoval do `%LOCALAPPDATA%` – okno se proto vytváří
  v kódu s `data_directory(...)`.
* **Cesty v DB jsou relativní ke kořeni archivu** s lomítky
  (`Games/System Shock 2`, `../GOG_Installers/Game_A`). Při startu se
  skládají s aktuálním kořenem, takže změna písmene jednotky nebo přesun na
  jiný počítač nic nerozbije. Absolutní cesta se uloží jen tehdy, když je
  složka na jiném disku než archiv (UI na to upozorní).
* **Identifikace archivu**: `.gamecurator-id` (UUID) + stejné UUID v tabulce
  `settings`. Chybějící soubor se obnoví z DB, nesoulad (DB zkopírovaná
  z jiného archivu) se uživateli ukáže a nabídne se „adopce“. Nic se nemaže.
* Žádný registr, žádné AppData, žádná služba na pozadí. Po zavření okna se
  zastaví případný sken, databáze se čistě zavře a proces skončí.
* WebView2: kontrola přes `tauri::webview_version()` ještě před vytvořením
  okna; při absenci nativní MessageBox s vysvětlením. Volitelně lze vedle exe
  přiložit „Fixed Version“ runtime do složky `WebView2Runtime`.
* WebView2 se spouští s přepínači `--disable-background-networking`,
  `--disable-component-update`, `--disable-domain-reliability`,
  `--disable-sync`, `--no-pings`, malou diskovou cache a bez cache shaderů.
  Aplikace je offline a stránky jsou uvnitř exe, takže to šetří paměť, CPU
  i zápisy na USB disk. (Jediná výjimka je hledání údajů o hrách na pokyn
  uživatele – viz „Údaje o hrách z internetu“; to dělá Rust, ne WebView2.)

## Vzhledy (Classic, Terminal CRT Amber, Matrix Terminal, DOS Midnight)

* Výhradně CSS: `<html data-theme="classic|amber|matrix|dos">` (+ třída `term`
  pro textové vzhledy). HTML, JavaScript i ovládání jsou ve všech
  vzhledech stejné; E2E test to ověřuje porovnáním všech ovládacích prvků.
* **Classic** (výchozí) je „VGA launcher“ – spouštěče a instalační programy
  DOSových her z počátku 90. let ve fialové paletě archivu: pixelové písmo
  **Pixel Operator** (Jayvee Enaguas, CC0), tlačítka a rámy s 2px zkosením
  a černým obrysem, lišty s proužky (jako titulky oken té doby), ostré
  stíny bez rozmazání, rozmítané (dithered) výplně posuvníků a překryvu,
  pixelové zaškrtávací políčka a přepínače (malá SVG v CSS), pixel-art
  ikona police (`ui/img/shelf-px.svg`) a krabice na místě chybějícího obalu
  (`ui/img/box-px.svg`). `retro.css` drží rozvržení a základní paletu,
  `classic.css` vše přestyluje a každé pravidlo má předponu
  `html[data-theme="classic"]`, takže terminálové vzhledy zůstávají beze změny.
  * Text je světle šedý (`--text: #c3bfcb`), ne bílý – menší kontrast.
  * **Volba písma** (Settings > Appearance, `config.classicFont`, atribut
    `html[data-classic-font]`): Pixel Operator (výchozí), Dogica, Geist Pixel,
    Roboto Slab. Velikosti v `classic.css` jsou proměnné (`--fs`, `--lh`,
    `--fs-title`, `--fs-small`, `--fs-big`, `--fs-mono`), takže každé písmo
    má svou mřížku: Dogica je 8px písmo (text 8 px, nadpisy 16 px, české
    znaky s háčky/kroužkem z Pixel Operator 8 na stejné mřížce, symboly
    rozhraní z `GC Pixel Symbols 8`); Geist Pixel a Roboto Slab jsou hladké
    ve 14 px. Logo zůstává vždy v Pixel Operator 8. Volbu ukládá i
    `theme-boot.js` (localStorage), takže se použije před prvním vykreslením.
    Licence: Dogica OFL 1.1 s rezervovanými názvy → soubory TTF beze změny;
    Geist Pixel OFL 1.1 bez rezervovaného názvu → oříznutý na řez Regular
    (3,6 MB → 26 kB WOFF2); Roboto Slab Apache 2.0 → WOFF2.
  * Nadpisy, tučné písmo a názvy her jsou oranžové (`--heading`), názvy her
    nainstalovaných na tomto PC zelené (`--installed`, třída `.game-title.inst`
    na kartě, v katalogu, v profilu i v deníku), aby se odlišily od bílého
    textu. Rám okna (lišty panelů, skupiny, záhlaví tabulek) zůstává fialový.
  * Pixelové písmo je ostré jen v násobcích své mřížky: Pixel Operator
    (a Bold, Mono) v 16 px, Pixel Operator 8 v 8, 16 nebo 24 px (titulky ve
    dvojnásobku). Výšky řádků jsou celé sudé pixely (20 px), aby znaky
    neležely na půlpixelech. Žádné jiné velikosti se v Classic nepoužívají.
  * Písmo nemá kurzívu a `font-synthesis: none` vypíná umělou kurzívu
    i tučnost; kurzíva v deníku se proto zobrazí fialovou barvou.
  * Webová písma (`ui/fonts/*.woff2`, každé ~5 kB) generuje
    `tools/make_fonts.py` z originálních TTF v `tools/fonts-src/`. Doplňuje
    symboly, které písmo nemá a rozhraní používá (▶ ◄ ✕ ＋ ★ ✎ ← → ↻ …),
    nakreslené přímo na jeho pixelové mřížce, aby nepadaly na systémové písmo.
* **Barevné role terminálových vzhledů**: místo jedné barvy fosforu má každý
  vzhled role `--t-fg` (běžný text a rámy), `--t-hi` (zvýraznění, varování),
  `--t-strong` (tučné písmo, nadpisy), `--t-title` (názvy her), `--t-key`
  (popisky tlačítek) a `--t-bar` / `--t-on-bar` (inverzní lišty, aktivní
  položka menu, titulky oken). **Matrix**: tlumená zelená `#6cc48f` + bílá
  (nadpisy) + žlutá `#eed27a` (názvy her, tlačítka). **Amber**: tlumená
  jantarová `#e3a04a` + teplá bílá (nadpisy, tlačítka) + světle rudá
  `#f2826f` (názvy her, zvýraznění). Lišty mají klidnější střední tón než
  text, záře a scanlines jsou slabší – vzhledy tolik „neřežou do očí“.
* **DOS Midnight** používá písmo IBM VGA – **PxPlus IBM VGA8** (VileR,
  CC BY-SA 4.0), beze změny, jen převedené do WOFF2. Deset symbolů rozhraní,
  které písmo nemá (▶ ✕ ＋ ★ ✎ ↻ …), kreslí samostatné malé písmo
  `GC VGA Symbols` na stejné mřížce 8×16 (CSS ho uvádí hned za VGA písmem,
  prohlížeč si z něj vezme jen chybějící znaky). Veškerý text je v nativních
  16 px s řádky 20 px a bez prostrkání, jako v textovém režimu.
* Terminálové vzhledy přemapují barevné proměnné klasického vzhledu na
  svou paletu a přestylují komponenty: monospace písmo (Cascadia Mono /
  Consolas; DOS Midnight písmo IBM VGA), lišty v inverzním režimu, dvojité „box-drawing“ rámečky,
  `[TLAČÍTKA]`, statické scanlines, jemná záře. Matrix má navíc statickou
  texturu „digitálního deště“ (`ui/img/matrix-rain.svg`, ~32 kB, generuje
  `tools/make_matrix_rain.py`). **DOS Midnight** sdílí stejný základ, ale
  s barvami textového režimu MS-DOS ztlumenými do tmavé modré (`#00001e`),
  šedým textem, žlutými zvýrazněními, azurovými lištami výběru, tlačítky
  `< OK >` a ostrým černým stínem pod okny – bez scanlines a bez záře.
  Žádné animace ani blikání.
* Výkon: celoobrazovkový překryv se scanlines a CSS filtry na obalech
  snižovaly scrollování mřížkou bez GPU na ~16 fps. Proto jsou scanlines
  jen na statických částech okna a obaly se tónují průhlednou vrstvou
  (skutečné barvy ukáže najetí myší).
* Volba se ukládá do `Data/config.json` (`theme`). Malý synchronní skript
  `js/theme-boot.js` ji navíc čte z cache ve WebView profilu ještě před
  prvním vykreslením, takže při startu nic neprobliká.

## Databáze

Schéma: [`crates/curator-core/src/schema.sql`](../crates/curator-core/src/schema.sql)
(kopie v [`docs/schema.sql`](schema.sql)). Tabulky ze zadání + několik
sloupců navíc (zdůvodnění):

| Tabulka / sloupec | Proč |
|---|---|
| `games.folder_key` UNIQUE | normalizovaná cesta – brání duplicitám (skener i ruční přidání) |
| `games.title_search` | název bez diakritiky a malými písmeny → „zaklinac“ najde „Zaklínač“ |
| `games.thumbnail_path` | náhled pro mřížku vedle plného obalu |
| `games.is_available` | označení nedostupné složky (záznam se nemaže) |
| `games.date_modified` | evidence změn |
| `files.size_bytes`, `is_missing`, `last_seen` | soubor zmizel → označí se, záznam zůstává |
| index `idx_files_one_primary` | nejvýš jeden hlavní instalátor na hru |
| `journal_entries.entry_date` | ze zadání: datum zápisku (výchozí dnešek, měnitelné) |
| `games.cover_crop` | výřez obalu (JSON `x, y, w, h` ve zlomcích obalu; NULL = na střed) |
| `installations` | kde je hra nainstalovaná na konkrétním počítači (klíč hra + název PC) |
| `screenshots` | screenshoty hry, volitelně patřící zápisku deníku |
| `additions` | dodatečné soubory hry (kopie v archivu nebo odkaz) |

**Verze 2 schématu** odstranila stav hry (Not Played, Completed…),
hodnocení her i zápisků a příznak oblíbených. Game Curator je archiv, ne
backlog, a tyto údaje nenápadně tlačí hry dokončovat. Databáze z verze 1 se
při prvním otevření převede (`ALTER TABLE … DROP COLUMN` v jedné transakci).
Předtím se vždy zapíše záloha `Data/Backups/library-*-pre-upgrade.db`,
takže se nic neztratí. **Verze 3** přidala sloupec `games.cover_crop`
a tabulku `installations`; **verze 4** tabulky `screenshots` a `additions`;
**verze 5** sloupce `games.icon_path`, `files.display_name`
a `additions.display_name`.
Převod ze starší verze proběhne postupně v jedné transakci, opět se zálohou
předem.

Cizí klíče jsou zapnuté (`PRAGMA foreign_keys = ON`), `ON DELETE CASCADE`
pro data navázaná na hru. Smazání hry vždy předchází potvrzovací dialog,
který vypíše, co se z DB odstraní (zápisky, záznamy souborů) a že složka
zůstane; zápisky lze ponechat jako HTML kopie v `Data/Journals`.

Odolnost: `journal_mode = DELETE` (žádný `-wal` soubor, který by se při
kopírování dal zapomenout), `synchronous = FULL` (vytažení USB nepoškodí DB),
každá vícekroková změna v transakci, před každým přístupem kontrola, že
`library.db` existuje. I/O chyba zahodí spojení, příští volání ho otevře
znovu. Migrace přes `PRAGMA user_version`.

Zálohy: `VACUUM INTO` do `Data/Backups/library-YYYYMMDD-HHMMSS-<typ>.db`,
po zápisu `PRAGMA quick_check`. Automaticky nejvýš jednou za 24 h při startu
(nastavitelné, rotace posledních N automatických), ručně v Settings. Obnova:
ověření zálohy → bezpečnostní záloha aktuálního stavu → výměna souboru →
nové otevření. Selže-li cokoliv, zůstane původní DB.

## Skener

1. Složka s `.exe`/`.msi`/`.bin` přímo uvnitř = kandidát (podsložky jako
   extras, soundtrack patří k ní).
2. Instalátory v generických podsložkách (`installer`, `extras`, `win`…)
   → kandidátem je rodičovská složka.
3. Složka jen s archivy (`zip/7z/rar`) = slabší kandidát (není předvybraný).

Název: `goggame-*.info` → `title.txt`/`game.txt`/… → název složky
(očištěný o `[GOG]`, verze, `_`) → název GOG instalátoru. Porovnání s DB:
stejná složka nebo složka uvnitř existující hry = „In catalog“ (nelze
importovat), stejný název = „Possible duplicate“ (varování). Běží
v samostatném vlákně, průběh přes událost `scan-progress` (max. 10×/s),
zrušení přes `AtomicBool` kontrolovaný u každé složky. Skener jen čte.

**Nápověda k hledání složek** (`scanner.js helpHtml()`): stručný průvodce na
stránce skeneru – jakou složku vybrat (hlavní složku sbírky, ne jednu hru),
jedna hra = jedna složka s ukázkovým stromem, jak se hra pozná (pravidla 1–3
výše a zdroje názvu), co znamená hloubka hledání a stavy po skenování.
Je otevřená, dokud nejsou výsledky; po skenování ustoupí (výsledky se
odrolují do zobrazení), ručně otevřená nebo zavřená zůstane, jak ji
uživatel nechal (`localStorage` `gc-scan-help`). Pod výběrem hloubky je
ukázková cesta, kam až skener dosáhne (`Hry\…\…\Hra`). Formulář hry má
u „Složka hry“ tlačítko **?** se stejným vysvětlením pro jednu hru, průvodce
prvního spuštění krátké „Jakou složku vybrat“ a Nastavení popisek u složky
ke skenování.

## Údaje o hrách z internetu

Jediná část aplikace, která jde na internet – a jen když uživatel stiskne
**Fetch details from …** nebo **Search again** (sekce „Game details from the
internet“ na stránce Archive Scanner). Uživatel volí zdroj (`config.detailsSource`:
`gog` / `dbher` / `moby`); všechny tři sdílejí `curator_core::lookup`:

* **Povolené adresy podle zdroje** (`lookup::allowed_url(source, …)`): jen
  `https`, výchozí port, bez jména a hesla, a jen hostitelé daného zdroje –
  GOG: `gog.com`, `*.gog.com`, `*.gog-statics.com`; Databáze-her.cz:
  `databaze-her.cz`, `www.databaze-her.cz`; MobyGames: `api.mobygames.com`.
  Kontroluje se každá adresa i po přesměrování. Zdroj tak nemůže poslat dotaz
  jinam (ani obal z cizí adresy).
* **Fetcher:** `ureq` + rustls s přibalenými kořenovými certifikáty Mozilla,
  User-Agent `GameCurator/<verze>`, žádné cookies ani účet, časové limity 10 s
  spojení / 30 s celkem, odpověď max. 8 MB (obrázek 15 MB). Pauza mezi dotazy
  podle zdroje: GOG 150 ms, Databáze-her.cz 1 s, MobyGames 1,1 s (API povoluje
  1 dotaz za sekundu a 360 za hodinu). Chyby `LookupError {source, kind}` mají
  srozumitelné hlášky; výpadek sítě, „zpomal“ (429), odmítnutý klíč nebo chybějící
  klíč zastaví celý běh. Klíč se v chybových hláškách nikdy neukáže (`redact`).
* **Uvedení zdroje:** popis z Databáze-her.cz dostane na konec řádek
  „— Zdroj: Databáze-her.cz, <adresa stránky> (licence CC BY-NC-ND 3.0 CZ)“
  (licence to vyžaduje), popis z MobyGames „— Source: MobyGames, <adresa>“.
  Popis se nijak neupravuje kromě převodu HTML na text.

### GOG.com

* **Odkud:** veřejný katalog obchodu GOG
  (`catalog.gog.com/v1/catalog?query=like:<slova názvu>&productType=in:game,pack&locale=en-US`)
  dává název, vývojáře, vydavatele, žánry, tagy, datum vydání a adresu
  svislého obalu. Popis z `api.gog.com/v2/games/<id>` (pole `description`),
  když chybí, ze staršího `api.gog.com/products/<id>?expand=description`
  (`full`, jinak `lead`; ten má limit ~200 dotazů/h, při „429“ se popis
  prostě vynechá). Obal z `images.gog-statics.com`. Jde o neoficiální,
  nedokumentovaná rozhraní, která používá samotný obchod GOG.
* **Kam smí:** klient (`curator_core::gog::Client`, `ureq` + rustls
  s přibalenými kořenovými certifikáty Mozilla, takže nezávisí na nastavení
  PC) kontroluje každou adresu i po přesměrování: jen `https`, výchozí port,
  hostitel `gog.com` / `*.gog.com` / `*.gog-statics.com`, bez jména a hesla.
  Posílá se jen hledaný název, User-Agent `GameCurator/<verze>`, žádné
  cookies ani účet. Mezi dotazy 150 ms pauza, časové limity 10 s spojení /
  30 s celkem, odpověď max. 8 MB (obrázek 15 MB). Stránky aplikace samy na
  internet nesmí (CSP), všechno dělá Rust.
* **Párování:** název se normalizuje (malá písmena, bez diakritiky, bez ™®©,
  „&“ = „and“, „Baldur's“ = „Baldurs“). Shoda 100 = stejný název, 92 = stejný
  bez „The“ a slov edice (Enhanced/Definitive/GOTY/Complete/Director's Cut…),
  pak podle společných slov. **Found** = nejlepší výsledek ≥ 90 a žádný jiný
  produkt stejného druhu se stejným skóre; jinak **Check the match** (nejlepší
  výsledek od 40 předvybraný, ale hra není zaškrtnutá) nebo **Not found**.
  Když první hledání nic jistého nenajde, zkusí se název bez „The“ a edice.
  „Search again“ hledá slovy uživatele; skóre je pak nejlepší z názvu hry
  i hledaných slov. Výběr jiného nalezeného produktu načte jeho údaje.
* **Revize a uložení:** pro každou hru se ukáže, co by se změnilo (stejné
  hodnoty se vynechají). **Prázdná pole jsou zaškrtnutá, vyplněná ne** –
  nic, co uživatel napsal, se nepřepíše bez zaškrtnutí. Žánry se spojí čárkou
  (max. 100 znaků), tagy se jen přidávají (bez opakování žánrů, max. 10 nových,
  celkem nejvýš 30), rok jen z data vydání 1940–2100, popis z HTML na čistý
  text (odstavce, odrážky, entity; max. 20 000 znaků). Obal jen hře, která
  žádný nemá (náhled v revizi jako `data:` PNG, uloží se jako WebP stejnou
  cestou jako ručně vybraný). `details_apply` dostane od stránky jen id hry,
  id produktu a názvy polí; hodnoty bere z údajů, které drží Rust.
* **Rozsah:** hry s chybějícími údaji (popis, žánr, vývojář, vydavatel, rok,
  tagy nebo obal), všechny hry, nebo po importu ve skeneru právě
  importované hry. Běží ve vlákně (`details-progress`, `details-finished`), lze
  zrušit; když zdroj není dostupný, skončí srozumitelnou hláškou a nic se
  nemění. Soubory her se nikdy nečtou ani neupravují.

### Databáze-her.cz (české popisy)

* **Bez vyhledávání:** vyhledávání webu (`/hledani/`) zakazuje `robots.txt`,
  takže se nepoužívá. Adresa stránky hry se odvodí z názvu stejně jako na webu
  (`/hry/<název>/`: malá písmena, bez diakritiky, „&“ = „and“, apostrofy
  a tečky pryč, ostatní znaky „-“) a zkusí se i varianta s římskými číslicemi
  („Baldurs Gate 3“ → `baldurs-gate-3`, `baldurs-gate-iii`); nejvýš 5 adres
  na hru. Do „Search again“ lze vložit adresu stránky hry (nebo jen její část
  za `/hry/`) – přečte se přímo. Tlačítko **Search on Databáze-her.cz ↗**
  otevře hledání webu v prohlížeči uživatele (`open_url`, jen `https`).
* **Co se čte ze stránky:** JSON-LD `VideoGame` (název, žánry, vývojáři),
  `div.game-title` (h1 a další názvy v h4, počítají se i pro shodu),
  `div.game-dates` (datum `dd.mm.rrrr`, přednostně PC), `dl.game-genres`
  (podžánry → tagy, „nezařazeno“ se vynechá), `div.game-description` (popis),
  „Rozsah“ (datadisk/DLC = druh produktu). Vydavatele ani obal stránka
  nedává, takže je zdroj nenabízí.

### MobyGames (anglické popisy)

* **API s klíčem uživatele:** `https://api.mobygames.com/v1`, klíč
  (`config.mobygamesKey`, Data/config.json) jde v parametru `api_key`.
  Bez klíče se nic neposílá. Hledání `/games?title=…&format=normal&limit=10`,
  vývojář a vydavatel z vydání pro PC (`/games/<id>/platforms/<platforma>`,
  přednost Windows 3, DOS 2, Linux 5…). Do „Search again“ lze vložit adresu
  hry na mobygames.com (`/game/<id>/…`) nebo její číslo.
* **Žánry:** kategorie „Basic Genres“ → žánr, ostatní (perspektiva, téma,
  styl…) → tagy, bez obchodního modelu, ovládání, add-onů a podobně. Obal se
  nebere.

## Bezpečnost

* Open/Run dostávají jen `game_id` + `file_id`; cesta se vezme z DB, ověří
  se, že je relativní bez `..`, a `canonicalize` potvrdí, že leží uvnitř
  složky hry (chrání i proti symlinkům).
* `Open` odmítne spustitelné typy (`.exe .msi .bat .cmd .lnk .ps1 .vbs …`).
  `Run` povolí jen `.exe`/`.msi` a vyžaduje `confirmed: true` – UI předtím
  vždy zobrazí potvrzovací dialog. Na Windows se používá `ShellExecuteW`
  s pracovní složkou instalátoru (UAC funguje normálně).
* Protokol `gcimg://` vrací jen soubory z `Data/Artwork/` a `Data/Thumbnails/`.
* Čtečka PDF a přehrávač čtou jen soubory, které Rust vyřešil z katalogu
  (stejná kontrola „uvnitř složky hry“ jako Open) a zaregistroval pod
  tokenem; stránka cestu nikdy nezná. JavaScript z PDF se nespouští.
* Text deníku se ukládá jako HTML z whitelistu (p, h2, h3, ul, ol, li, b, i,
  u, blockquote) bez atributů; sanitizuje se při uložení i zobrazení. CSP
  zakazuje inline skripty.
* Kód nikdy nemaže, nepřesouvá ani neupravuje nic mimo vlastní `Data/`.
* Na internet jde jen hledání údajů o hrách, a jen po stisku tlačítka:
  HTTPS jen na adresy zvoleného zdroje (GOG.com, Databáze-her.cz nebo
  api.mobygames.com), kontrola každé adresy i po přesměrování (viz výše).
  Odkazy „Page ↗“ / „Search on … ↗“ otevírá prohlížeč uživatele; Rust je
  předtím ověří proti adresám zdroje.
