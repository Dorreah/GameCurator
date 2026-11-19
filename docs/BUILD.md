# Sestavení Game Curator

Game Curator je aplikace **jen pro Windows 10/11 (x64)**. Jiné systémy se
nepodporují; build na Linuxu existuje pouze jako testovací harness pro
automatické E2E testy (viz konec dokumentu) a bez featury `e2e` ho build
skript odmítne.

## Doporučeno: nativně na Windows → jeden soubor

1. Nainstalujte Rust (https://rustup.rs, toolchain `stable-x86_64-pc-windows-msvc`)
   a **Visual Studio Build Tools** s komponentou „Desktop development with C++“.
2. V kořeni projektu:

   ```powershell
   cargo build --release -p game-curator
   ```

   Výsledek: `target\release\GameCurator.exe`. MSVC build linkuje zavaděč
   WebView2 staticky, takže je to **jediný soubor**. Node.js ani npm nejsou
   potřeba: frontend (čisté HTML/CSS/JS v `ui/`) se vloží do exe při
   kompilaci.

3. Portable balíček = složka `GAME_CURATOR` s `GameCurator.exe`
   a `README.txt` (z `portable\`). Složky `Data`, `Games` a soubor
   `.gamecurator-id` si aplikace vytvoří sama.

Vývoj na Windows:

```powershell
$env:GAME_CURATOR_ROOT = "D:\GC-dev"   # kořen archivu (jinak složka exe)
cargo run -p game-curator
```

Změny v `ui/` se projeví po novém `cargo build` (soubory se vkládají do exe).

## Co dělá binárku malou a lehkou

* release profil: `opt-level = "s"`, LTO, `codegen-units = 1`, `strip`,
  `panic = "abort"`;
* Tauri bez výchozích featur – jen `wry`, `compression`,
  `common-controls-v6` (žádný tray, menu, runtime ACL ani linuxové backendy);
* plugin dialogů bez linuxových backendů, žádný plugin pro otevírání souborů
  (Open/Run přímo přes `ShellExecuteW`);
* vyhledávání bez diakritiky přes vlastní malou tabulku místo Unicode
  normalizačních tabulek;
* WebView2 spouštěný bez síťových služeb na pozadí, synchronizace a velké
  diskové cache (méně paměti, CPU a zápisů na USB disk).

Výsledek (cross-build MinGW): **5,4 MB** místo předchozích 6,8 MB.

## Cross-build z Linuxu (MinGW-w64)

Tak vznikl přiložený `GameCurator.exe`. MinGW build zavaděč WebView2
linkuje dynamicky, proto vedle exe musí ležet `WebView2Loader.dll`
(redistribuovatelná knihovna Microsoftu; `tools/package.sh` ji přibalí).

```bash
sudo apt install mingw-w64 zip
rustup target add x86_64-pc-windows-gnu      # pokud máte rustup s přístupem k internetu
./tools/build-windows.sh
./tools/package.sh                            # dist/GAME_CURATOR + zip
```

Bez `rustup target add` skript použije `-Zbuild-std=std,panic_abort` se
zdrojáky standardní knihovny (`rustup component add rust-src`, nebo
distribuční balíček `rust-1.xx-src` nalinkovaný do
`$(rustc --print sysroot)/lib/rustlib/src/rust/library`). Ubuntu balíček má
z `library/std/Cargo.toml` vypuštěnou Windows závislost; je potřeba ji vrátit:

```toml
[target.'cfg(windows)'.dependencies.windows-link]
path = "../windows_link"
```

`.cargo/config.toml` linkuje runtime GCC staticky; exe kromě systémových DLL
importuje jen `WebView2Loader.dll` (ověřeno přes `objdump -p`).

## Testy

```bash
cargo test -p curator-core              # 18 unit + 20 integračních testů (libovolný OS)
cargo test -p curator-core --release t8 -- --nocapture   # výkon jádra (500 her)
```

Jádro (`crates/curator-core`) je čistý Rust bez GUI; pravidla cest jsou
napevno windowsová (nerozlišuje velikost písmen, `\` = `/`), takže testy na
libovolném systému ověřují chování na Windows.

### E2E testovací harness (Linux)

Skutečná aplikace na WebKitGTK ovládaná přes WebDriver. Jen pro testy,
nikdy pro distribuci (featura `e2e` nahrazuje nativní dialogy cestou ze
souboru `GC_E2E_PICK_FILE` a akce Open/Run zapisuje do `GC_E2E_ACTION_LOG`;
UI to ve stavovém řádku označí „test build“).

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential libxdo-dev librsvg2-dev libssl-dev \
                 xvfb webkit2gtk-driver
cargo install tauri-driver --locked
pip install selenium pillow
cargo build --release -p game-curator --features e2e
cargo test -p game-curator --features e2e
Xvfb :99 -screen 0 1400x900x24 &
python3 tests/e2e/run_e2e.py target/release/GameCurator /tmp/e2e-out
```

`src-tauri/tauri.linux.conf.json` obsahuje jen úpravu CSP pro WebKitGTK
(vlastní protokol `gcimg:`), do Windows buildu se nedostane.

## Pomocné skripty

* `tools/make_icons.py src-tauri/icons` – ikony (Pillow)
* `tools/make_matrix_rain.py ui/img/matrix-rain.svg` – statická textura
  „digitálního deště“ pro vzhled Matrix Terminal
