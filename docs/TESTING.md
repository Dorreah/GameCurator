# Výsledky testování – Game Curator 0.1

Datum: 19. 11. 2026 (večer) · prostředí: Ubuntu 24.04 (virtuální stroj, 2 vCPU,
bez GPU), Rust 1.97, WebKitGTK 2.52 jako testovací harness · Windows build:
cross-kompilace MinGW.

Shrnutí: **39/39 automatických testů jádra a aplikace a 133/133 kontrol
end-to-end prošlo** (včetně převodu databáze z verze 0.1, výřezu obalu, dělicí čáry profilu,
screenshotů, dodatečných souborů,
nainstalovaných her, kontrol minimalismu a 12 kontrol vzhledů (4 vzhledy)). Testovala se skutečná aplikace (Rust backend + UI),
ne mock. Všechny testy jsou v repozitáři a lze je zopakovat (`docs/BUILD.md`).

> ⚠️ **Na skutečném Windows aplikace nespuštěna** – viz
> [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) bod 1 a kontrolní seznam na
> konci tohoto dokumentu.

## 1. Testy jádra (`cargo test -p curator-core`, `cargo test -p game-curator --features e2e`)

18 unit testů (cesty s pravidly Windows, čištění názvů, vyhledávání bez
diakritiky přes vlastní tabulku – čeština, němčina, polština, francouzština,
rumunština, rozložené znaky, cyrilice –, kategorie souborů, odhad
instalátoru, data, názvy záloh, párovací klíče nainstalovaných her, edice
za pomlčkou, názvy z instalačních souborů, bezpečná jména kopií) + 1 test
protokolu `gcimg` + 20 integračních testů (`crates/curator-core/tests/acceptance.rs`):

| Test | Ověřuje |
|---|---|
| `t1_t2_archive_survives_move_and_drive_letter_change` | archiv vytvořen na „D“, přejmenován na „E“ → hry (uvnitř `Games/` i `../GOG_Installers`), artwork, zápisky dostupné; ID archivu sedí |
| `archive_id_file_is_restored_and_mismatch_detected` | chybějící `.gamecurator-id` se obnoví z DB; cizí ID → nesoulad → adopce |
| `t3_game_management_never_touches_the_folder` | přidání, artwork, úprava, štítky, odebrání; výpis souborů složky před a po je identický |
| `duplicates_and_validation` | duplicitní složka, prázdný název, neexistující složka, složka `Data`, špatný rok |
| `library_search_sort_facets_and_file_stats` | hledání „zaklinac“ → „Zaklínač 3“, escapování `%`, žádné filtry (staré klíče filtrů v JSON se ignorují), řazení podle roku/velikosti/data přidání, počty pro stavový řádek, počet a velikost souborů na hru (chybějící se nepočítají), „What I know“ |
| `database_v1_is_upgraded_with_a_backup_and_no_data_loss` | databáze z verze 0.1 (se stavem, hodnocením, oblíbenými): převod na schéma 3 (výřez obalu, tabulka instalací), sloupce stavu odstraněny, název, poznámky, štítky i zápisky zachovány, `integrity_check` ok, záloha `pre-upgrade` obsahuje původní data, druhé otevření už nic nezálohuje |
| `t4_archive_content_categories_primary_and_safety` | kategorie (EXE ≠ automaticky instalátor), vlastní kategorie, jediný hlavní instalátor, `..` i symlink mimo složku odmítnuty |
| `missing_files_and_unavailable_folders_keep_their_records` | smazaný soubor → `is_missing`, odpojená složka → nedostupná, žádný záznam nesmazán, editace offline hry funguje |
| `t5_journal_persists_across_restarts` | CRUD, řazení, filtr podle hry, HTML kopie, přetrvání po restartu |
| `t6_scanner_detects_imports_selected_and_avoids_duplicates` | 6 typů kandidátů (GOG složka, instalátor v podsložce `installer`, vnoření, `title.txt`, `goggame-*.info`, jen ZIP), import výběru, opakovaný sken = „in catalog“, žádné duplicity, „possible duplicate“ |
| `scanner_can_be_cancelled_and_handles_single_game_root` | zrušení, sken jedné herní složky |
| `t7_unplugged_database_is_reported_never_deleted` | archiv zmizí za běhu → `ArchiveUnavailable`, po návratu data celá |
| `artwork_is_optimized_and_validated` | 2400×3600 PNG → WebP ≤1200×1600 + náhled přesně 360×480, WebP vstup, odmítnutí GIF a poškozeného souboru, úklid sirotků, protokol nevydá `library.db` |
| `artwork_crop_is_rendered_and_can_be_changed` | široký obrázek (vlevo červená, vpravo modrá): výchozí výřez na střed = obě barvy; výřez zvolený při přidání = jen červená, první náhled smazán; úprava v profilu = jen modrá; „celý obrázek“ = tmavé pruhy nahoře a dole; úprava hry bez obrázku výřez zachová; neplatné výřezy (NaN, mimo obrázek, nulová šířka) odmítnuty; hra bez obrázku nemá co upravit |
| `screenshots_gallery_and_journal` | PNG → WebP kopie + náhled 480×270, originál nezměněn (bajt po bajtu), 5000 px zmenšeno na 3840, neobrázek odmítnut, nejnovější první, popisek (čištění, max. 500 znaků), připojení k zápisku (zobrazí se u zápisku i v jeho HTML kopii s relativním odkazem), jen k zápisku stejné hry, odpojení = zůstane v galerii, přesun zápisku k jiné hře vezme screenshot s sebou, smazání zápisku screenshot ponechá, smazání screenshotu smaže jen kopii, úklid při startu nikdy nesmaže obrázek, odebrání hry: ve výchozím stavu screenshoty zůstanou, na volbu se smažou |
| `additional_files_are_copied_or_linked` | kopie do `Data/Additions/game-<id>/` (shodný obsah, originál zůstává, volné jméno „(2)“), odkaz mimo archiv = absolutní cesta, uvnitř archivu = relativní (cestuje s diskem), duplicitní odkaz odmítnut, složka / neznámý druh / neexistující hra odmítnuty, řazení podle druhu, úprava poznámky, zmizelý odkazovaný soubor = „not found“, nic se nemaže, `.exe` = Run, odebrání: odkaz nikdy nesmaže soubor, kopie jen na požádání, odebrání hry s volbou smaže jen kopie |
| `installed_games_are_detected_per_computer` | GOG záznam s ™ v názvu a `goggame-*.info` (program + argumenty, úloha s `..\` ignorována), záznam Windows s programem z `DisplayIcon`; „Gothic II“ ≠ „Gothic“, „Mafia: …“ ≠ „Mafia“, chybějící složka = nenalezeno; jiný počítač nic nevidí; ruční „Locate…“ (ne-.exe odmítnuto); po odinstalaci zmizí automatický záznam, ruční zůstane; chybějící program → srozumitelná chyba, nic se nemaže; „Forget“; odebrání hry smaže i záznamy instalací, soubory zůstanou |
| `backups_auto_prune_and_restore` | auto záloha max 1×/24 h, rotace, ruční zálohy se nemažou, obnova + bezpečnostní záloha, vadná záloha nic nezmění |
| `config_roundtrip_and_corruption` | uložení/načtení včetně vzhledu, starší `config.json` bez vzhledu → Classic, zrušené volby (úvodní obrazovka, řazení podle vývojáře) neresetují zbytek nastavení, neplatný vzhled odmítnut, šířka levého sloupce profilu (výchozí 42,5 %, oříznutí na 25–75 %), poškozený soubor → výchozí + `.bad` kopie |
| `t8_performance_with_500_games` | výkon dotazů (níže) |

## 2. End-to-end testy aplikace (`tests/e2e/run_e2e.py`)

Skutečná aplikace (release build s featurou `e2e`) ovládaná přes WebDriver
(tauri-driver + WebKitWebDriver) pod Xvfb. Nativní dialogy nahrazuje soubor
s cestou, Open/Run/Play se zapisují do logu místo spuštění, seznam
nainstalovaných programů a název počítače přichází ze souboru (`GC_E2E_INSTALLED`,
`GC_E2E_MACHINE`). **133/133 OK.**

**Relace 0 – databáze z verze 0.1**
archiv s databází schématu 1 (hra „Completed“, hodnocení 5, oblíbená,
zápisek s hodnocením, starý `config.json` s `startScreen` a řazením podle
vývojáře) → aplikace startuje normálně · před převodem vznikne záloha
`pre-upgrade` · sloupce stavu, hodnocení a oblíbených jsou pryč · poznámky
a zápisky zachovány.

**Relace 1 – první spuštění a správa her (testy 3, 4, 5, 6 ze zadání)**
průvodce (3 kroky) · vytvoření `.gamecurator-id`, `Data/library.db`,
`Data/WebView` (profil WebView na archivu) · uložení dokončení průvodce ·
**uvítací okno** při startu (archivář a badatel, prožitek místo hodin
a achievementů, soustředění, analýza, detaily) před průvodcem, „Don't show
this again“ uloženo do `config.json` · uvítací panel prázdné knihovny · Add Game: výběr složky + návrh názvu, obal →
WebP náhled (< 60 kB) zobrazený přes `gcimg://` · **editor výřezu** (rámeček 3:4,
posuvník, Fit, Fill): široký obrázek tažením posunut k levému okraji → uložen
výřez `x = 0` a náhled obsahuje jen levou polovinu · karta s názvem, rokem a obalem ·
**Adjust** v profilu → „Fit“ → uloženo: celý obrázek s tmavými pruhy, starý
náhled nahrazen ·
profil (hra, archiv, „What I know about this game“, deník) · soubory
seskupené podle druhu (Installers, Installer data, Manuals, Music, Archives…)
se souhrnem „6 files · … 1 manual …“ · žádné ovládání stavu, hodnocení ani
oblíbených · předvybraný hlavní instalátor · **Open PDF** → otevřeno · **Open Folder**
soundtracku → otevřeno · **Run EXE** → nejdřív potvrzovací dialog, spuštění
až po potvrzení · vlastní kategorie uložena · úprava popisu · přidání štítku
(stávající zachovány) · „What I know about this game“ · zápisek s nadpisem, odstavcem
a seznamem, datum = dnešek · úprava zápisku · HTML kopie v `Data/Journals` ·
**skener**: 4 hry nalezeny, složka Docs ignorována, názvy očištěné · import
3 z 4 (s rokem a obalem) · opakovaný sken = 3× „In catalog“, nelze znovu vybrat ·
žádné duplicity v DB · **zrušení běžícího skenu** (40 000 složek) · hledání
(„1 of 4“) · **žádné filtry** (stav, žánr, štítek, dostupnost) · řazení podle
roku a názvu · karty ukazují rok a velikost na disku · Catalog View: název,
rok, žánr, počet souborů, velikost, složka (bez stavu) · **levé menu**: žádná
horní lišta, 5 položek s SVG ikonami v pořadí Library, Gaming Journal,
Archive Scanner, Settings, About · druhý zápisek napsaný v profilu jiné hry ·
**stránka Gaming Journal** seskupí oba zápisky po hrách (novější hra nahoře,
„1 entry“, „2 entries · 2 games“), nemá filtr ani New entry, klik na hru
otevře její profil · **About**: hlavní myšlenka, motto, verze, umístění a ID
archivu, zvýrazněná položka menu · Settings už údaje z About neduplikují ·
**nainstalované hry**: „Look for installed games again“ najde Disco Elysium
podle GOG záznamu „Disco Elysium - The Final Cut“ (7-Zip ignorován) ·
zaškrtávátko na všech kartách, zaškrtnuté jen u nainstalované hry · sloupec
v katalogu · profil ukazuje složku instalace · **Play** je v rámečku
„Installed on this PC“ (ne v hlavičce) → nejdřív dialog
„Start game?“, spuštění programu z `goggame-*.info` až po potvrzení ·
„Show location“ otevře složku instalace · nenalezená hra: „Not installed“,
bez Play → „Locate…“ → program vybrán, Play k dispozici · **dělicí čára
profilu**: tažením rozšířen levý sloupec, šířka uložena do `config.json`
a platí i v dalším profilu, šipky ji posunou, dvojklik vrátí výchozí ·
About zmiňuje DRM-free disky, vlastní tempo a „Gaming history deserves
careful, loving treatment.“ · **screenshoty**: panel pod deníkem s výzvou,
4 obrázky najednou → 3 nejnovější náhledy + „All 4 screenshots“, uložené
kopie, originály beze změny · náhled otevře velký obrázek (1920 px),
popisek uložen Enterem, šipky listují, mřížka celé galerie · smazání
s potvrzením smaže jen kopii · screenshot přidaný v editoru zápisku patří
zápisku (i v HTML kopii) a je i v galerii hry · z náhledu u zápisku se
galerie otevře s informací o zápisku · **dodatečné soubory**: panel pod
Archive s vysvětlením · soubor mimo archiv se ve výchozím stavu kopíruje
(obsah shodný, originál zůstává) · velký soubor jen odkázán (nic se
nekopíruje) · řazení fix → patch → bonus · `.exe` patch: Run až po
potvrzení · Open a Folder · odebrání kopie se smazáním, odkazovaný soubor
se nikdy nesmaže · **drag & drop**: obrázek přetažený na Screenshots se
přidá, složka odmítnuta, puštění mimo cílovou plochu jen s nápovědou ·
tažení nad panelem ho zvýrazní s popiskem · dva soubory přetažené na
Additional files = jeden dialog pro oba · odebrání
hry: dialog uvádí, že složka nebude smazána → složka a soubory zůstaly ·
ruční záloha DB · **vzhledy**: 4 volby v Settings, výchozí Classic,
Amber, Matrix i DOS Midnight se okamžitě použijí a uloží do `config.json`, ovládací prvky
jsou ve všech vzhledech identické (porovnání všech tlačítek, polí a výběrů),
monospace písmo, screenshoty knihovny, profilu, dialogu, deníku a skeneru.

**Relace 2 – přesun archivu / změna písmene jednotky (testy 1, 2, 5)**
celá „jednotka“ D přejmenována na E a spuštěno jako **jiný počítač** (jiný
název PC, žádné hry nainstalované): žádná hra se neukazuje jako
nainstalovaná · screenshoty se načtou z nové jednotky · uvítací okno se už
neukáže · bez průvodce ·
zvolený vzhled
(Matrix) obnoven při startu · všechny hry dostupné ·
obaly se načtou · zápisek a poznámky zachovány po restartu · externí složka
`../GOG_Installers` se správně přeložila na nový disk.

**Relace 3 – nedostupný disk (test 7)**
složka `GOG_Installers` „odpojena“: 2 hry označené OFFLINE, nic nesmazáno ·
zpět na prvním počítači: nalezená i ručně ukázaná instalace jsou znovu známé ·
stavový řádek „2 offline“ · profil vysvětluje nedostupnost · záznamy souborů
zachovány · **`Data/` odebrána za běhu** → stavový řádek „ARCHIVE DRIVE NOT
AVAILABLE“ → po vrácení vše funguje · `PRAGMA integrity_check` = ok.

**Relace 4 – výkon s 500 hrami (test 8)**
500 herních složek naskenováno a importováno přes UI skeneru, každá hra
dostala obal, znovu spuštěno a změřeno · lazy loading · po zavření okna
neběží žádný proces.

## 3. Naměřené hodnoty (test 8)

Release build. Linux/WebKitGTK, softwarové vykreslování bez GPU.

| Měření | Hodnota |
|---|---|
| Velikost aplikace | Windows `GameCurator.exe` **5,7 MB** (+ `WebView2Loader.dll` 160 kB u MinGW buildu) |
| Start aplikace → zobrazeno 500 karet s obaly (vč. zjištění nainstalovaných her) | **0,8 s** (DOM připraven → karty: ~0,6 s) |
| Sken 500 složek (UI, vč. vyhodnocení) | 0,6 s |
| Import 500 her přes skener (vč. indexace souborů, `synchronous=FULL`) | 2,6 s |
| Vyhledávání nad 500 hrami (vstup → překreslení, **včetně 140ms debounce**) | **~150 ms**, samotný SQL dotaz 0,14 ms |
| Přepnutí Grid → Catalog (500 řádků) | ~170 ms |
| Scrollování mřížkou 500 karet, Classic, 1. průchod (dekódování obalů) | **29 fps** |
| Scrollování, Classic, 2. průchod | **35 fps** |
| Scrollování, Terminal CRT Amber | **25 fps** |
| Scrollování, Matrix Terminal | **23 fps** |
| Scrollování, DOS Midnight (bez scanlines) | **32 fps** |
| Lazy loading obalů | po startu načteno 18 z 500; zbytek až při scrollu |
| Dotazy jádra (`cargo test --release t8`) | výpis 500 her vč. počtu a velikosti souborů 0,51 ms · hledání 0,15 ms · řazení podle velikosti 0,63 ms · počty 0,19 ms · kontrola dostupnosti 500 složek 1,3 ms · sken 500 složek 5 ms |
| RAM v klidu, 3 hry | proces aplikace 165 MB, celkem s WebKit procesy 388 MB |
| RAM v klidu po startu, 500 her | 166 MB / 404 MB |
| RAM po proscrollování všech 500 obalů (a přepnutí všech vzhledů) | 171 MB / 564 MB |
| Procesy po zavření | 0 |

Čísla scrollování kolísají mezi běhy a mezi instancemi testovacího virtuálu
(jen 2 jádra, bez GPU, softwarové vykreslování): mřížka knihovny se v
posledních kolech neměnila, a přesto vycházela Classic 29–57 fps, Amber
24–44, Matrix 23–35. Výše je poslední běh. Rozhodující bude měření na
Windows s GPU.

**Jak číst čísla o paměti:** tvoří je téměř celá WebKitGTK v tomto
prostředí (GTK, Mesa llvmpipe pro softwarové vykreslování, dekódované
obrázky). Logika aplikace v Rustu je malá. WebView2 na Windows má jiný
procesový model a čísla tam budou jiná. Je potřeba je změřit na Windows
(kontrolní seznam níže).

**Optimalizace, které testy odhalily a které jsou zapracované:**
* minimalistický výchozí vzhled (plochá tmavá paleta, tenké rámečky) zrychlil
  scrollování ve všech vzhledech: Classic 44 → 57 fps, Amber 35 → 44,
  Matrix 28 → 35;
* terminálové vzhledy: první verze scrollovala na 15–16 fps. Měření
  ukázalo, že vinu nese celoobrazovkový překryv se scanlines a CSS filtry na
  obalech. Scanlines jsou teď jen na statických částech okna a obaly tónuje
  průhledná vrstva: 35 / 28 fps. Déšť v Matrixu stojí ~4 fps a zůstal jako
  hlavní znak vzhledu. Bez GPU WebKitGTK zřejmě překresluje celé okno;
  WebView2 vrstvy vždy skládá, takže na Windows bude rozdíl menší;
* chyba v UI: třída `.info` profilu kolidovala s `.notice.info`, takže
  informační hlášky (např. ve skeneru) se lámaly do sloupců. Opraveno;
* chyba: po návratu dříve nedostupných složek zůstal v postranním panelu
  starý počet „Unavailable“. Opraveno;
* scrollování 15 → 46–49 fps: ploché pozadí stránky místo texturovaného
  gradientu (bez GPU se překresloval v každém snímku), `content-visibility:
  auto` pro karty mimo obraz, náhledy zmenšeny ze 400×600 na 320×480;
* chyba v UI skeneru: sken menší složky skončil dřív, než přišla odpověď
  na příkaz ke spuštění, a UI zůstalo ve stavu „Starting…“. Opraveno a
  pokryto testem.

## 4. Co testy nepokrývají

* běh na Windows 10/11 a WebView2 (viz níže),
* skutečné nativní dialogy pro výběr souboru nebo složky (v E2E nahrazené),
* skutečné spuštění instalátoru, hry a UAC (v E2E se jen zapíše do logu),
* skutečné čtení registru Windows (v E2E se seznam programů načte ze souboru;
  kód pro registr je jen přeložený a typově zkontrolovaný),
* fyzické odpojení USB disku (simulováno přejmenováním složek).

## 5. Kontrolní seznam pro první test na Windows

1. Rozbalte `GameCurator-0.1.0-windows-portable.zip` na USB disk, spusťte
   `GameCurator.exe` → průvodce. Ověřte, že v `%LOCALAPPDATA%` nevznikla
   složka `cz.gamecurator.archive` a vše je v `GAME_CURATOR\Data`.
2. Přidejte 2–3 hry (jednu v `Games\`, jednu v `X:\GOG_Installers\…`),
   nahrajte obal, napište zápisek, zavřete.
3. Ve Správě disků změňte písmeno jednotky (nebo připojte disk k jinému PC)
   a spusťte znovu → vše dostupné (testy 1 a 2).
4. V profilu: Open PDF, Open Folder, Run instalátoru → objeví se potvrzení
   a pak UAC (test 4).
5. Odpojte disk s herní složkou → hra OFFLINE; odpojte disk s archivem za
   běhu → hláška ve stavovém řádku, po připojení a znovuspuštění data celá
   (test 7).
6. Správce úloh: paměť v klidu; po zavření okna nesmí zůstat proces
   `GameCurator.exe` ani `msedgewebview2.exe` patřící aplikaci (test 8).
   Ve všech třech vzhledech zkuste plynulost scrollování knihovnou.
7. Na PC bez WebView2 Runtime (např. Windows 10 LTSC) → srozumitelná
   chybová zpráva, žádné změny na disku.
8. Nainstalované hry: na PC s hrou nainstalovanou z GOG instalátoru (např.
   `C:\GOG Games\…`) by po startu měla mít karta zaškrtávátko; v profilu
   Show location a Play (hra se spustí po potvrzení, se správnou pracovní
   složkou – vyzkoušejte i hru přes DOSBox). Hru, kterou detekce nenašla,
   ukažte přes Locate…
9. Výřez obalu: vyberte široký obrázek (např. 16:9), posuňte ho a přibližte,
   uložte; v profilu Adjust → Fit → Save.
