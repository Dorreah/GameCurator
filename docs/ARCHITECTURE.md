# Game Curator 0.1 – architektura a struktura projektu

Cílová platforma: **jen Windows 10/11 x64** s WebView2. Kód pro jiné systémy
neexistuje; jádro je čistý Rust (testovatelný kdekoli) a linuxový build
aplikace slouží výhradně jako testovací harness (`--features e2e`).

## Přehled vrstev

```
┌───────────────────────────────────────────────────────────────┐
│ ui/  (HTML + CSS + vanilla JavaScript, ES moduly, bez frameworku)│
│   app.js  router, stavový řádek, klávesové zkratky               │
│   css/retro.css (Classic) + css/themes.css (Amber, Matrix, DOS)  │
│   library / profile / journal / scanner / settings / about /    │
│   wizard; levé menu s ikonami je jediná navigace (bez top baru) │
│   api.js  jediné místo, které volá Tauri IPC                    │
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
│   screenshots.rs screenshoty (galerie, vazba na zápisek deníku)  │
│   additions.rs   dodatečné soubory: kopie do archivu / odkaz     │
│   scanner.rs  hledání kandidátů, zrušení, import bez duplicit    │
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
* **Library** má jen hledání podle názvu a řazení (`LibraryQuery { search,
  sort }`). Filtry nejsou ani v UI, ani v jádře; staré klíče v JSON se
  ignorují. Stavový řádek ukazuje „N games · M offline“ (`facets.js`).
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

## Struktura repozitáře

```
game-curator/
├── Cargo.toml                 workspace (core + aplikace), release profil (LTO, strip)
├── .cargo/config.toml         linker pro cross-build na Windows (MinGW)
├── crates/curator-core/
│   ├── src/*.rs               jádro (viz výše)
│   ├── src/schema.sql         SQLite schéma (verze 4)
│   └── tests/acceptance.rs    integrační testy = akceptační testy 1–8 na úrovni jádra
├── src-tauri/
│   ├── Cargo.toml             binárka GameCurator, featura `e2e` jen pro testy
│   ├── tauri.conf.json        CSP, žádná okna z configu (okno se tvoří v kódu)
│   ├── tauri.linux.conf.json  jen CSP pro testovací harness (WebKitGTK)
│   ├── capabilities/          minimální oprávnění JS
│   ├── icons/                 ikony (generuje tools/make_icons.py)
│   └── src/                   lib.rs, commands.rs, system.rs, protocol.rs, state.rs
├── ui/                        index.html, css/retro.css + themes.css, js/*.js, img/
├── portable/README.txt        návod, který se přikládá k GameCurator.exe
├── tests/e2e/run_e2e.py       end-to-end testy přes WebDriver (tauri-driver)
├── tools/                     build-windows.sh, package.sh, make_icons.py, make_matrix_rain.py
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
  i zápisy na USB disk.

## Vzhledy (Classic, Terminal CRT Amber, Matrix Terminal, DOS Midnight)

* Výhradně CSS: `<html data-theme="classic|amber|matrix|dos">` (+ třída `term`
  pro textové vzhledy). HTML, JavaScript i ovládání jsou ve všech
  vzhledech stejné; E2E test to ověřuje porovnáním všech ovládacích prvků.
* Terminálové vzhledy přemapují barevné proměnné klasického vzhledu na
  paletu fosforu a přestylují komponenty: monospace písmo (Cascadia Mono /
  Consolas), lišty v inverzním režimu, dvojité „box-drawing“ rámečky,
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
a tabulku `installations`; **verze 4** tabulky `screenshots` a `additions`.
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

## Bezpečnost

* Open/Run dostávají jen `game_id` + `file_id`; cesta se vezme z DB, ověří
  se, že je relativní bez `..`, a `canonicalize` potvrdí, že leží uvnitř
  složky hry (chrání i proti symlinkům).
* `Open` odmítne spustitelné typy (`.exe .msi .bat .cmd .lnk .ps1 .vbs …`).
  `Run` povolí jen `.exe`/`.msi` a vyžaduje `confirmed: true` – UI předtím
  vždy zobrazí potvrzovací dialog. Na Windows se používá `ShellExecuteW`
  s pracovní složkou instalátoru (UAC funguje normálně).
* Protokol `gcimg://` vrací jen soubory z `Data/Artwork/` a `Data/Thumbnails/`.
* Text deníku se ukládá jako HTML z whitelistu (p, h2, h3, ul, ol, li, b, i,
  u, blockquote) bez atributů; sanitizuje se při uložení i zobrazení. CSP
  zakazuje inline skripty.
* Kód nikdy nemaže, nepřesouvá ani neupravuje nic mimo vlastní `Data/`.
