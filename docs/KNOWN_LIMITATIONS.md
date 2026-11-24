# Známá omezení – Game Curator 0.2

## Ověření na Windows

1. **Aplikace nebyla spuštěna na skutečném Windows 10/11.** Windows build
   vznikl cross-kompilací (MinGW) na Linuxu. Funkční a akceptační testy
   běžely proti stejnému kódu na Linuxu (WebKitGTK místo WebView2). Kód
   specifický pro Windows se zkompiloval, ale v reálném Windows neběžel:
   `ShellExecuteW` (Open/Run, UAC u instalátorů), `GetDriveTypeW`,
   `MessageBoxW`, profil WebView2 v `Data\WebView`, další okna (čtečka PDF,
   přehrávač) se stejným profilem WebView2. Pokus o spuštění pod Wine
   selhal už při inicializaci samotného Wine (`wineboot` padá v testovacím
   virtuálu, s aplikací to nesouvisí). U exe je tedy ověřena jen jeho
   struktura: PE32+ GUI x86-64, importuje pouze systémové DLL
   a `WebView2Loader.dll`. **Před ostrým nasazením zopakujte na Windows testy
   1, 2, 4, 7 a 8 ze zadání** (postup v `docs/TESTING.md`).
2. **MinGW build potřebuje `WebView2Loader.dll`** vedle `GameCurator.exe`
   (je v balíčku; redistribuovatelná knihovna Microsoftu). Nativní MSVC build
   na Windows (`docs/BUILD.md`) ji linkuje staticky a je to jediný soubor.
3. **Jen Windows 10/11 x64.** Starší Windows, 32bit ani jiné systémy nejsou
   podporovány a kód pro ně neexistuje (záměr kvůli velikosti a jednoduchosti).
4. Naměřené hodnoty výkonu a paměti pocházejí z linuxového testovacího
   harnessu (WebKitGTK, softwarové vykreslování ve virtuálu bez GPU). Na
   Windows s WebView2 se budou lišit; plynulost scrollování bude s GPU
   typicky lepší, hlavně u terminálových vzhledů.

## Vědomé odchylky od původního zadání

* **Bez stavu hry, hodnocení a oblíbených.** Zadání požadovalo filtry Not
  Played / Playing / Completed / On Hold / Favorites, stav na kartě
  a vlastní hodnocení. Na přání byly odstraněny: aplikace má pomáhat
  s pořádkem v archivu a ke kurátorské práci, ne tlačit k dohrávání. Vlastní
  skupiny her lze popsat štítky. Databáze z verze 1 se převede, předtím se
  vždy uloží záloha.
* **Filtr jen podle žánru a štítků.** Zadání počítalo s filtry podle stavu,
  žánru, štítků a dostupnosti. Filtry podle stavu a dostupnosti byly na
  přání odstraněny; od verze 0.2 knihovna filtruje podle žánru a štítků
  (tlačítko s trychtýřem vedle Sort). Počet nedostupných (OFFLINE) her
  ukazuje stavový řádek a karty jsou označené.
* **Bez horní lišty.** Jediné menu je levý sloupec s ikonami (Library,
  Gaming Journal, Archive Scanner, Settings, About). Informace o aplikaci
  a archivu se přesunuly ze Settings na stránku About.
* **Gaming Journal** se píše jen v profilu hry. Stránka Gaming Journal
  v menu je přehled všech zápisků seskupený po hrách (bez filtru a bez
  tlačítka New entry).
* **Čtení registru.** Zadání zakazuje registr; aplikace do něj nic
  nezapisuje. Kvůli detekci nainstalovaných her ale **čte** záznamy GOG
  a seznam nainstalovaných programů Windows. Nic se neukládá mimo `Data/`.
* Panel „Archive Notes“ se jmenuje **„What I know about this game“**.
* Průvodce prvním spuštěním má 3 kroky (Welcome, Your games, Done).
  Nastavení už nemá volbu úvodní obrazovky; aplikace vždy začíná v knihovně
  a výchozí řazení si pamatuje z panelu nástrojů.

## Funkce

* **Jazyk rozhraní**: angličtina a čeština (od 0.2). Překládá se rozhraní
  a zprávy aplikace; data uživatele (názvy her, popisy, poznámky, žánry,
  tagy) a údaje stažené z internetu zůstávají, jak jsou – GOG.com
  a MobyGames posílají anglické texty, Databáze-her.cz české. Datum se v obou jazycích píše „22. 11. 2026“, velikosti
  souborů s desetinnou tečkou (1.3 MB). Chybové hlášky knihoven třetích stran
  (např. pdf.js u poškozeného PDF, WebView2) a hlášky Windows mimo známé
  kódy chyb zůstanou anglicky. Okno nabídky zápisu, které už je otevřené,
  přepne jazyk okamžitě, jeho titulek v liště Windows ale až při dalším
  otevření. Čeština je psaná s tykáním.
* **Nápověda ke skeneru** popisuje pravidla rozpoznání her (instalátor
  přímo ve složce, generické podsložky, jen archivy). Sbírky uspořádané
  jinak (např. všechny instalátory v jedné společné složce bez podsložek
  pro hry) skener rozpozná jako jednu „hru“ – takové hry je lepší přidat
  ručně nebo nejdřív rozdělit do složek.
* **Screenshoty** se ukládají jako WebP s kvalitou 90 (do 3840 px), ne jako
  bezeztrátové originály – pro vzpomínky stačí a zabírají zlomek místa.
  Originál zůstává, kde byl. Obrázky jde vybrat v dialogu nebo přetáhnout
  z Průzkumníka; vložení ze schránky (Ctrl+V) zatím není.
* **Drag & drop** je v E2E ověřený přes stejnou cestu v kódu (příkaz
  `e2e_drop`), ne skutečným tažením myší ve Windows – to ověří až test na
  Windows (přepočet pozice při škálování displeje).
* **Dodatečné soubory**: kopírování velkého souboru nemá ukazatel průběhu
  (jen „Copying…“). Odkaz mimo archiv je absolutní cesta, na jiném počítači
  proto soubor bývá „not found“ – záznam se tím nemaže.
* **Ikony her v zobrazení Catalog** se berou z `goggame-*.ico` ve složce
  instalace (GOG), jinak z ikony spouštěného `.exe`. Hra bez obojího (nebo
  s neznámým programem) ikonu nemá. Ikona se načte jednou a pamatuje se;
  když se změní, obnoví ji „Locate…“. Čtení ikon z `.exe` je ověřené na
  uměle sestavených programech a na sestaveném `GameCurator.exe`, ne na
  sbírce skutečných her.
* **Nabídka zápisu po dohrání** funguje jen pro hry spuštěné tlačítkem
  ▶ Play a jen dokud je Game Curator otevřený. Konec hry se pozná podle
  toho, že už neběží žádný program ze složky instalace; hra spuštěná přes
  program mimo složku (např. samostatně nainstalovaný ScummVM nebo DOSBox)
  se sleduje jen přes první spuštěný proces. Hra, která běžela kratší dobu
  než 15 s (pád při startu, zavřený launcher), nabídku nevyvolá. Program
  ze složky instalace, který po hře zůstane běžet (např. pomocník nebo
  nástroj otevřený zvlášť), konec hry oddálí, dokud neskončí. Okno nabídky
  je vždy navrchu; nad hrou v exkluzivním režimu celé obrazovky se ukázat
  nemůže, ale to už hra skončila. Chování na skutečném Windows zatím není
  ověřené (testováno na Linuxu s launcherem, který hned skončí, a programem
  ve složce instalace).
* **Písmo Dogica** nemá české znaky s háčkem a kroužkem (ě š č ř ž ů …):
  doplňuje je Pixel Operator 8 na stejné 8px mřížce, takže se mírně liší
  tvarem. Text v Dogice je malý (8 px); ostrý je při měřítku 100 % a 200 %.
  Geist Pixel nemá tučný řez (zvýraznění dělá barva).
* **Přejmenování souborů** mění jen název v katalogu Game Curatoru; soubor
  na disku (a v Průzkumníku) má dál původní název.
* **Pixelová písma vzhledů Classic a DOS Midnight** jsou dokonale ostrá při měřítku displeje
  Windows 100 % a 200 % (jeden pixel písma = 1 nebo 2×2 pixely obrazovky).
  Při 125 % a 150 % připadá na pixel písma 1,25 nebo 1,5 pixelu obrazovky,
  takže některé tahy vyjdou o pixel širší a hrany jsou lehce měkčí – text
  zůstává dobře čitelný. Kdo chce ostré písmo i při takovém měřítku, může
  zvolit Amber nebo Matrix. Pixelová písma nemají kurzívu ani (VGA) tučný řez:
  zvýraznění se zobrazí barvou. V DOS Midnight má veškerý text jednu velikost
  (16 px), jako skutečný textový režim.

* **Detekce nainstalovaných her** hledá jen v záznamech GOG a v seznamu
  programů Windows. Hry nainstalované bez instalátoru (rozbalené ZIP,
  přenesené složky) nenajde – ty lze ukázat ručně tlačítkem „Locate…“.
  Párování je záměrně přísné; hra s jiným názvem v systému než v katalogu se
  nenajde automaticky. Spuštění přes Steam/Galaxy klienta se neřeší (spouští
  se přímo program hry).
* Starší náhledy obalů (2:3 z předchozích verzí) zůstávají, dokud se obal
  neupraví; rámeček je ořízne na střed jako dřív.

5. **Artwork = jen obal hry.** Složky `Artwork/Backgrounds` a `Artwork/Logos`
   se vytváří podle zadané struktury, ale verze 0.2 je ještě nepoužívá.
6. **Rozhraní je v angličtině**, protože zadání pojmenovává obrazovky
   a tlačítka anglicky (Library, Add Game, Open Folder…). Dokumentace je
   česky. Lokalizace by byla nová funkce.
7. Žánr je jedno textové pole (max. 100 znaků), více žánrů se odděluje
   čárkou. Filtr v knihovně je bere jednotlivě, ale celé: „Open“ nenajde
   „Open world“. Výběr filtru se po zavření aplikace nepamatuje.
8. Skener: když leží instalátory více her **přímo ve zvolené složce** (bez
   podsložek), vznikne jeden kandidát. Kandidáta nelze rozdělit. Hloubka
   hledání 1–5 úrovní.
9. Obnovit lze jen zálohu ze složky `Data/Backups` (s názvem
   `library-YYYYMMDD-HHMMSS-*.db`). Je to záměr kvůli bezpečnosti; cizí soubor
   tam lze nakopírovat ručně.
10. Výpis obsahu herní složky je omezen na 3 000 položek a hloubku 4 (UI
    zobrazí upozornění), aby profil zůstal rychlý i u obří složky.
11. Terminálové vzhledy obaly her jen **tónují** průhlednou vrstvou, nejsou
    tedy čistě jednobarevné. Skutečný monochromatický filtr výrazně zpomaloval
    scrollování. Bez GPU scrollují terminálové vzhledy o něco pomaleji než
    Classic (v posledním měření 31–33 fps proti 40–52; čísla mezi běhy kolísají).
12. Editor deníku používá `document.execCommand` (zastaralé API, ale ve
    WebView2 i WebKitu stále podporované). Formátování: odstavce, 2 úrovně
    nadpisů, odrážky, číslování, tučné, kurzíva, podtržení, citace.
11a. **Údaje z GOG.com stojí na neoficiálních rozhraních obchodu GOG.**
    Katalog (`catalog.gog.com`) i produktová data (`api.gog.com`) nejsou
    veřejně dokumentovaná a GOG je může změnit; pak hledání přestane
    fungovat, dokud se aplikace neupraví (nic se tím nepoškodí). Tvar
    odpovědi katalogu je ověřený podle skutečné odpovědi, ale aplikace sama
    se při vývoji ke skutečnému GOG.com nepřipojila (testovací prostředí
    nemá přístup na internet; testuje se proti místní napodobenině). Hledá se
    podle názvu, takže hra s výrazně jiným názvem se musí dohledat přes
    „Search again“ a hry, které GOG neprodává, se nenajdou. Popisy jsou
    anglicky; starší produktové API má limit kolem 200 dotazů za hodinu
    (popis se pak vynechá). Proxy nastavená ve Windows se nepoužívá (platí
    pro všechny tři zdroje).
11b. **Databáze-her.cz se čte z veřejných stránek her, bez vyhledávání.**
    Web nemá API a své vyhledávání zakazuje robotům (`robots.txt`), takže
    Game Curator adresu stránky odvodí z názvu (a zkusí římské číslice).
    Hra s jiným názvem než na webu (podtitul, jiný překlad, „The Final Cut“)
    se tak nenajde – adresu její stránky je potřeba vložit do „Search again“
    (pomůže „Search on Databáze-her.cz ↗“). Údaje se čtou z HTML stránky:
    když web změní vzhled, čtení přestane fungovat, dokud se aplikace
    neupraví. Při vývoji byla ověřena struktura skutečných stránek
    v prohlížeči, ale aplikace sama se ke skutečnému webu nepřipojila.
    Vydavatele ani obal zdroj nenabízí. Popisy jsou pod licencí
    CC BY-NC-ND 3.0 CZ: jsou jen pro vlastní nekomerční katalog a zdroj
    pod popisem by se neměl mazat.
11c. **MobyGames potřebuje vlastní API klíč.** Klíč se získá po přihlášení
    na mobygames.com (profil → API); MobyGames ho může podmínit placeným
    MobyPro. Limit 360 dotazů za hodinu znamená asi 150–180 her za hodinu
    (na hru 1–3 dotazy); po překročení se hledání zastaví s hláškou a stačí
    to zkusit později. Klíč je v `Data/config.json` čitelně (jako zbytek
    nastavení přenosné aplikace) – kdo má přístup k disku, může ho přečíst.
    Aplikace se ke skutečnému MobyGames API při vývoji nepřipojila (bez
    klíče a internetu); tvar odpovědí je podle dokumentace API. Obal se
    z MobyGames nebere.
12a. **Čtečka PDF je na čtení, ne na práci s PDF.** Umí stránky, zoom
    a listování; neumí vyhledávání v textu, výběr a kopírování textu,
    obsah (záložky), odkazy, formuláře, tisk ani PDF chráněné heslem pro
    otevření (nabídne „Open in default app“). JavaScript v PDF se záměrně
    nespouští. Kreslí přes pdf.js, takže vzácně se nějaký neobvyklý PDF může
    zobrazit jinak než v Acrobatu – pak pomůže „Open in default app“.
12b. **Přehrávač načte celou stopu do paměti** (kvůli okamžitému přetáčení
    i na pomalém disku). U běžných MP3/FLAC soundtracků jde o jednotky až
    desítky MB; soubory nad 1 GB přehrávač odmítne. Které formáty přehraje,
    určuje WebView2 (Chromium): MP3, AAC/M4A, OGG Vorbis/Opus, FLAC a WAV ano;
    WMA, MIDI ani moduly (MOD/XM) ne – ty jdou přes „Open in default app“.
    Přehrávání v testech ověřeno na WebKitGTK (GStreamer), na WebView2
    neověřeno (bod 1). Hudba se neukládá do historie, není ekvalizér ani
    přehrávání více her najednou (jeden přehrávač, nový soundtrack ho přepne).

## Provoz

13. Import velkého množství her na pomalém USB flash disku: každá hra se
    potvrzuje samostatně s `synchronous = FULL` (několik fsync na hru), aby
    odpojení disku nemohlo poškodit data. Lokálně trval import 500 her
    jednotky sekund, na pomalém flash disku to mohou být desítky sekund.
14. Když se odpojí disk, **na kterém leží samotné exe**, může Windows
    aplikaci ukončit (kód se načítá z disku). Databáze zůstane konzistentní
    (transakce + `synchronous = FULL`). Odpojení se ve stavovém řádku projeví
    při návratu do okna nebo nejpozději do 30 s.
15. Neexistuje zámek proti dvěma spuštěným instancím. SQLite souběh zvládne
    bezpečně, ale jedna instance neuvidí změny druhé bez obnovení.
16. Standardní souborový dialog Windows si sám pamatuje naposledy použité
    složky v registru uživatele. Dělá to Windows Shell, ne Game Curator.
17. `Data/WebView` (mezipaměť WebView2, jednotky MB) lze kdykoli smazat.
18. Spuštění instalátoru, který vyžaduje práva správce, zobrazí běžný
    dialog UAC (řeší `ShellExecuteW`). Na reálném Windows neověřeno (bod 1).
