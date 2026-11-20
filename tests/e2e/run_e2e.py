#!/usr/bin/env python3
"""
End-to-end acceptance tests for Game Curator (spec section 14, tests 1-8).

Drives the real application (Rust backend + WebView UI) through WebDriver:
tauri-driver + WebKitWebDriver on Linux. The app must be built with the
`e2e` feature: native file dialogs then read the "picked" path from
GC_E2E_PICK_FILE and Open/Run are written to GC_E2E_ACTION_LOG instead of
being executed.

Usage:  python3 tests/e2e/run_e2e.py <path-to-GameCurator-binary> <output-dir>
Needs:  Xvfb on :99, tauri-driver, WebKitWebDriver, selenium, Pillow.
"""
import json
import re
import os
import shutil
import socket
import sqlite3
import subprocess
import sys
import time
import traceback
from pathlib import Path

from PIL import Image
from selenium import webdriver
from selenium.common.exceptions import StaleElementReferenceException, TimeoutException
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.options import ArgOptions
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.support.ui import WebDriverWait

BIN = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()
WORK = OUT / "work"
PICK = WORK / "pick.txt"
LOG = WORK / "actions.log"
INSTALLED = WORK / "installed.json"
RESULTS = []
METRICS = {}


# ------------------------------------------------------------------ fixtures

def touch(p: Path, size=64):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(b"x" * size)


def game_folder(p: Path, slug: str):
    touch(p / f"setup_{slug}_1.0.2.exe", 4096)
    touch(p / f"setup_{slug}_1.0.2-1.bin", 16384)
    real_pdf(p / "Manual.pdf", slug.replace("_", " ").title())
    real_audio(p / "Soundtrack" / "01 - Main Theme.mp3", 4, 440)
    real_audio(p / "Soundtrack" / "02 - Combat.flac", 3, 660)
    touch(p / "Extras" / "artbook.zip", 256)


def real_pdf(path: Path, title="System Shock 2", pages=3):
    """A small but real PDF manual: a title, text in a standard font (needs the
    viewer's font data) and a dark box per page, A4/Letter mixed sizes."""
    path.parent.mkdir(parents=True, exist_ok=True)
    objs = []
    kids = []
    font_id = 3
    objs.append(None)  # 1 catalog
    objs.append(None)  # 2 pages
    objs.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
    for n in range(1, pages + 1):
        w, h = (612, 792) if n != 2 else (595, 842)
        text = f"{title} - Manual page {n}".encode("latin-1")
        stream = (b"0.1 0.1 0.1 rg 72 %d 300 120 re f BT /F1 28 Tf 72 %d Td (%s) Tj ET"
                  % (h - 340, h - 120, text))
        objs.append(b"<< /Length %d >>\nstream\n%s\nendstream" % (len(stream), stream))
        content_id = len(objs)
        objs.append(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d] /Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>"
                    % (w, h, font_id, content_id))
        kids.append(len(objs))
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objs[1] = b"<< /Type /Pages /Kids [%s] /Count %d >>" % (b" ".join(b"%d 0 R" % k for k in kids), len(kids))
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, o in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n%s\nendobj\n" % (i, o)
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
    path.write_bytes(bytes(out))


def real_audio(path: Path, seconds=4, freq=440):
    """A real music file (sine tone) made with ffmpeg: mp3 / flac / ogg by extension."""
    path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"sine=frequency={freq}:duration={seconds}",
                    "-ac", "2", "-ar", "44100", str(path)], check=True)


def wide_cover(path: Path, left, right, size=(1600, 1000)):
    """Wide picture: left half one colour, right half another (checkered)."""
    img = Image.new("RGB", size)
    for y in range(0, size[1], 40):
        for x in range(0, size[0], 40):
            base = left if x < size[0] // 2 else right
            c = tuple(min(255, v + 40) for v in base) if (x // 40 + y // 40) % 2 == 0 else base
            img.paste(c, (x, y, x + 40, y + 40))
    img.save(path)


def scene(path: Path, colour, seed):
    """A made-up 'screenshot': dusk gradient, a horizon and a few lit windows."""
    from PIL import ImageDraw
    w, h = 1920, 1080
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=tuple(int(c * (0.25 + 0.75 * t) * (1 - 0.5 * t) + 20 * t) for c in colour))
    d.rectangle([0, int(h * 0.72), w, h], fill=tuple(c // 6 for c in colour))
    for k in range(9):
        x = 120 + ((k * 211 + seed * 97) % (w - 360))
        top = int(h * (0.35 + ((k * 37 + seed * 13) % 30) / 100))
        d.rectangle([x, top, x + 140, int(h * 0.72)], fill=tuple(c // 4 + 10 for c in colour))
        for wy in range(top + 20, int(h * 0.72) - 20, 46):
            for wx in range(x + 18, x + 130, 38):
                if (wx + wy + seed) % 3:
                    d.rectangle([wx, wy, wx + 16, wy + 22], fill=(235, 215, 150))
    d.ellipse([w - 420 - seed * 40, 120, w - 300 - seed * 40, 240], fill=(250, 240, 210))
    img.save(path)


def thumb_colour(path: Path, x, y):
    """'left' / 'right' / 'dark' at a pixel of a thumbnail (360x480)."""
    img = Image.open(path).convert("RGB")
    assert img.size == (360, 480), img.size
    r, g, b = img.getpixel((x, y))
    if max(r, g, b) < 60:
        return "dark"
    return "left" if b > r else "right"


def cover(path: Path, color, size=(900, 1200)):
    img = Image.new("RGB", size, color)
    for y in range(0, size[1], 40):
        for x in range(0, size[0], 40):
            if (x // 40 + y // 40) % 2 == 0:
                img.paste(tuple(min(255, c + 40) for c in color), (x, y, x + 40, y + 40))
    img.save(path)


# ------------------------------------------------------------------ harness

def free_port(port):
    with socket.socket() as s:
        return s.connect_ex(("127.0.0.1", port)) != 0


class App:
    def __init__(self, root: Path, label: str, machine: str = "E2E-PC", installed: Path = INSTALLED):
        for port in (4444, 4445):
            for _ in range(50):
                if free_port(port):
                    break
                time.sleep(0.1)
        env = os.environ.copy()
        env.update(
            GAME_CURATOR_ROOT=str(root),
            GC_E2E_PICK_FILE=str(PICK),
            GC_E2E_ACTION_LOG=str(LOG),
            GC_E2E_INSTALLED=str(installed),
            GC_E2E_MACHINE=machine,
            # a stand-in game started by Play: a launcher that quits at once and
            # leaves a program from the installation folder running for 2 s
            GC_E2E_GAME_CMD=str(WORK / "C" / "GOG Games" / "Disco Elysium" / "play.sh"),
            GC_E2E_FAST_SESSIONS="1",
            DISPLAY=":99",
            WEBKIT_DISABLE_COMPOSITING_MODE="1",
        )
        self.log = open(OUT / f"driver-{label}.log", "w")
        self.proc = subprocess.Popen(["tauri-driver"], env=env, stdout=self.log, stderr=subprocess.STDOUT)
        for _ in range(100):
            if not free_port(4444):
                break
            time.sleep(0.1)
        opts = ArgOptions()
        opts.set_capability("browserName", "wry")
        opts.set_capability("tauri:options", {"application": str(BIN)})
        t0 = time.perf_counter()
        self.d = webdriver.Remote(command_executor="http://127.0.0.1:4444", options=opts)
        self.launch_s = time.perf_counter() - t0
        self.d.set_window_size(1360, 860)
        self.w = WebDriverWait(self.d, 15)

    def quit(self):
        try:
            try:
                self.d.quit()
            except Exception:
                if not getattr(self, "closed", False):
                    raise
        finally:
            self.proc.terminate()
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.log.close()
            subprocess.run(["pkill", "-f", str(BIN)], check=False)
            time.sleep(0.5)

    # helpers
    def css(self, sel, timeout=15):
        return WebDriverWait(self.d, timeout).until(EC.presence_of_element_located((By.CSS_SELECTOR, sel)))

    def visible(self, sel, timeout=15):
        return WebDriverWait(self.d, timeout).until(EC.visibility_of_element_located((By.CSS_SELECTOR, sel)))

    def click(self, sel, timeout=15):
        el = WebDriverWait(self.d, timeout).until(EC.element_to_be_clickable((By.CSS_SELECTOR, sel)))
        el.click()
        return el

    def click_text(self, text, tag="button", timeout=15):
        el = WebDriverWait(self.d, timeout).until(
            EC.element_to_be_clickable((By.XPATH, f"//{tag}[contains(normalize-space(.), '{text}')]")))
        el.click()
        return el

    def gone(self, sel, timeout=15):
        WebDriverWait(self.d, timeout).until(EC.invisibility_of_element_located((By.CSS_SELECTOR, sel)))

    def js(self, script, *args):
        return self.d.execute_script(script, *args)

    def count(self, sel):
        return len(self.d.find_elements(By.CSS_SELECTOR, sel))

    def dismiss_welcome(self, dont_show=True):
        """The welcome window at start: close it (optionally for good)."""
        self.visible(".welcome")
        if dont_show:
            self.click("#wl-hide")
        self.click("#wl-enter")
        self.gone(".welcome")

    def drop(self, paths, selector):
        """Files dragged from Explorer onto the element (same path as a real drop)."""
        self.js("""const r=document.querySelector(arguments[1]).getBoundingClientRect();
                   window.__TAURI__.core.invoke('e2e_drop', {paths: arguments[0], x: r.left + r.width / 2, y: r.top + r.height / 2});""",
                [str(p) for p in paths], selector)

    def shot(self, name):
        self.d.save_screenshot(str(OUT / f"{name}.png"))

    def type(self, sel, text, clear=True):
        el = self.css(sel)
        if clear:
            el.clear()
        el.send_keys(text)
        return el

    def until(self, fn, timeout=15, msg=""):
        return WebDriverWait(self.d, timeout, ignored_exceptions=[StaleElementReferenceException]).until(lambda d: fn(), msg)

    def text(self, sel):
        """innerText via JS (robust against re-rendering and CSS containment)."""
        return self.js("const e=document.querySelector(arguments[0]); return e ? e.innerText : ''", sel)

    def new_window(self, known, timeout=15):
        """Waits for a window the app opened (PDF viewer, music player) and switches to it."""
        handle = WebDriverWait(self.d, timeout).until(lambda d: next((h for h in d.window_handles if h not in known), False))
        self.d.switch_to.window(handle)
        return handle

    def close_modals(self):
        for _ in range(4):
            if not self.count(".overlay"):
                return
            btns = self.d.find_elements(By.CSS_SELECTOR, ".overlay .window-foot .btn.gold, .overlay [data-ok]")
            if btns:
                btns[-1].click()
                time.sleep(0.2)


def wm_close(title_regex):
    """Asks windows to close the way their title-bar X does (WM_DELETE_WINDOW),
    so the app sees a real close request. Returns how many windows matched."""
    import ctypes
    import ctypes.util
    ids = subprocess.run(["xdotool", "search", "--name", title_regex], capture_output=True, text=True,
                         env={**os.environ, "DISPLAY": ":99"}).stdout.split()
    if not ids:
        return 0
    x = ctypes.cdll.LoadLibrary(ctypes.util.find_library("X11"))
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XInternAtom.restype = ctypes.c_ulong
    x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]

    class ClientMessage(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int), ("display", ctypes.c_void_p),
                    ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong), ("format", ctypes.c_int), ("data", ctypes.c_long * 5)]

    class XEvent(ctypes.Union):
        _fields_ = [("xclient", ClientMessage), ("pad", ctypes.c_long * 24)]

    x.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(XEvent)]
    x.XFlush.argtypes = [ctypes.c_void_p]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    dpy = x.XOpenDisplay(b":99")
    protocols = x.XInternAtom(dpy, b"WM_PROTOCOLS", 0)
    delete = x.XInternAtom(dpy, b"WM_DELETE_WINDOW", 0)
    for wid in ids:
        ev = XEvent()
        ev.xclient.type = 33  # ClientMessage
        ev.xclient.window = int(wid)
        ev.xclient.message_type = protocols
        ev.xclient.format = 32
        ev.xclient.data[0] = delete
        x.XSendEvent(dpy, int(wid), 0, 0, ctypes.byref(ev))
    x.XFlush(dpy)
    x.XCloseDisplay(dpy)
    return len(ids)


def pick(path):
    PICK.write_text(str(path) if path else "")


def pick_many(paths):
    PICK.write_text("\n".join(str(p) for p in paths))


def actions():
    return LOG.read_text().splitlines() if LOG.exists() else []


def check(name, cond, detail=""):
    RESULTS.append({"test": name, "ok": bool(cond), "detail": detail})
    print(("  PASS " if cond else "  FAIL ") + name + (f" - {detail}" if detail and not cond else ""))
    return cond


def section(title):
    print(f"\n== {title}")
    RESULTS.append({"section": title})


def rss_kb(binary_name="GameCurator"):
    """RSS of the app process plus all its descendants (WebKit processes)."""
    out = subprocess.run(["ps", "-eo", "pid=,ppid=,rss=,comm="], capture_output=True, text=True).stdout
    procs = {}
    for line in out.splitlines():
        parts = line.split(None, 3)
        if len(parts) == 4:
            procs[int(parts[0])] = (int(parts[1]), int(parts[2]), parts[3])
    roots = [p for p, (_, _, c) in procs.items() if c.startswith(binary_name[:15])]
    total, own, seen = 0, 0, set()
    stack = list(roots)
    while stack:
        p = stack.pop()
        if p in seen:
            continue
        seen.add(p)
        total += procs[p][1]
        if p in roots:
            own += procs[p][1]
        stack.extend(c for c, (pp, _, _) in procs.items() if pp == p)
    return own, total, len(seen)


# ------------------------------------------------------------------ tests

def run():
    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    drive_d = WORK / "D"
    root_d = drive_d / "GAME_CURATOR"
    root_d.mkdir(parents=True)
    gog = drive_d / "GOG_Installers"
    game_folder(root_d / "Games" / "System Shock 2", "system_shock_2")
    for name, slug in [("Disco_Elysium_[GOG]", "disco_elysium"), ("Baldurs_Gate_3", "baldurs_gate_3"),
                       ("Gothic (2001)", "gothic"), ("Heroes of Might and Magic 3", "homm3")]:
        game_folder(gog / name, slug)
    touch(gog / "Docs" / "readme.txt")
    wide_cover(WORK / "cover_ss2.png", (40, 80, 140), (200, 160, 50))
    # games installed on "drive C" of this test PC (the GOG record names an edition)
    de_inst = WORK / "C" / "GOG Games" / "Disco Elysium"
    touch(de_inst / "disco.exe")
    (de_inst / "goggame-1771589310.info").write_text(json.dumps({"gameId": "1771589310", "name": "Disco Elysium - The Final Cut",
        "playTasks": [{"category": "game", "isPrimary": True, "path": "disco.exe", "type": "FileTask"}]}))
    Image.new("RGBA", (48, 48), (230, 120, 20, 255)).save(de_inst / "goggame-1771589310.ico", sizes=[(48, 48), (32, 32)])
    shutil.copy("/bin/sleep", de_inst / "game-bin")
    # the stand-in launcher starts the "game" (2 s) and quits; the game's end is written down
    (de_inst / "play.sh").write_text('#!/bin/sh\nd="$(dirname "$0")"\n( "$d/game-bin" 2; date +%s.%N > "$d/ended" ) &\nexit 0\n')
    (de_inst / "play.sh").chmod(0o755)
    de_ico_bytes = (de_inst / "goggame-1771589310.ico").read_bytes()
    ss2_inst = WORK / "C" / "Games" / "SS2"
    touch(ss2_inst / "ss2.exe")
    # screenshots taken in a game, and files downloaded for it
    (WORK / "Pictures").mkdir(parents=True, exist_ok=True)
    for i, colour in enumerate([(200, 60, 60), (60, 160, 90), (70, 90, 200), (220, 180, 60), (150, 70, 170)], 1):
        scene(WORK / "Pictures" / f"shot{i}.png", colour, i)
    dl = WORK / "Downloads"
    dl.mkdir(parents=True, exist_ok=True)
    (dl / "widescreen_fix_1.3.zip").write_bytes(b"PK" + b"x" * 5000)
    (dl / "Making of System Shock 2.mkv").write_bytes(b"\x1aE" + b"v" * 9000)
    (dl / "ss2_community_patch.exe").write_bytes(b"MZ" + b"p" * 3000)
    INSTALLED.write_text(json.dumps([
        {"name": "Disco Elysium - The Final Cut", "installDir": str(de_inst), "source": "gog"},
        {"name": "7-Zip 24.08", "installDir": str(WORK / "C" / "7-Zip"), "source": "uninstall"},
    ]))
    cover(WORK / "cover_de.jpg", (140, 60, 60))

    # -------------------------------------------------- session 0: upgrade
    section("Session 0 - database from version 0.1 (play status, ratings) is upgraded")
    up_root = WORK / "U" / "GAME_CURATOR"
    (up_root / "Data").mkdir(parents=True)
    game_folder(up_root / "Games" / "Fallout 2", "fallout_2")
    v1 = sqlite3.connect(up_root / "Data" / "library.db")
    v1.executescript((Path(__file__).resolve().parents[2] / "crates/curator-core/tests/fixtures/schema_v1.sql").read_text())
    v1.executescript("""PRAGMA user_version = 1;
        INSERT INTO games (title, title_search, folder_path, folder_key, status, rating, is_favorite, archive_notes)
          VALUES ('Fallout 2', 'fallout 2', 'Games/Fallout 2', 'games/fallout 2', 'completed', 5, 1, 'Interplay big box, 1998.');
        INSERT INTO journal_entries (game_id, entry_date, title, content, rating) VALUES (1, '2026-03-01', 'Arroyo', '<p>temple</p>', 5);
        INSERT INTO settings (setting_key, setting_value) VALUES ('archive_id', '2b9d6e2a-0c5e-4d6f-9a51-7f1b2c3d4e5f');""")
    v1.commit()
    v1.close()
    (up_root / ".gamecurator-id").write_text("2b9d6e2a-0c5e-4d6f-9a51-7f1b2c3d4e5f\n")
    (up_root / "Data" / "config.json").write_text('{"wizardCompleted": true, "startScreen": "journal", "sort": "developer"}')
    app = App(up_root, "s0")
    try:
        app.dismiss_welcome()
        app.until(lambda: app.count(".card") == 1, msg="upgraded game visible")
        check("0.1 database opens; old settings do not break the start", "Fallout 2" in app.text(".card"))
        check("a backup is written before the upgrade", any((up_root / "Data" / "Backups").glob("library-*-pre-upgrade.db")))
        cols = [r[1] for r in sqlite3.connect(up_root / "Data" / "library.db").execute("pragma table_info(games)")]
        check("status, rating and favourite columns are gone", not ({"status", "rating", "is_favorite"} & set(cols)), str(cols))
        app.click(".card")
        app.until(lambda: "Interplay big box" in (app.css("#pf-notes").get_attribute("value") or ""))
        app.until(lambda: "Arroyo" in app.text("#pf-journal"))
        check("notes and journal entries kept", True)
    except Exception:
        traceback.print_exc()
        check("session 0 completed without errors", False, traceback.format_exc(limit=2))
        app.shot("error-s0")
    finally:
        app.quit()

    # -------------------------------------------------- session 1
    section("Session 1 - first run, wizard, game management (tests 3, 4, 5, 6)")
    app = App(root_d, "s1")
    try:
        app.visible(".welcome")
        welcome = app.text(".welcome")
        check("welcome window at start: what Game Curator is about",
              all(w in welcome for w in ["video game archivist and researcher", "experienced", "hours, achievements or numbers",
                                         "deep focus", "analysis", "noticing the small details"]), welcome[:300])
        check("welcome window offers “Don't show this again”", app.count("#wl-hide[type=checkbox]") == 1 and "Don't show this again" in app.text(".overlay"))
        check("the wizard waits until the welcome window is closed", app.count(".wizard") == 0)
        app.shot("00-welcome")
        app.dismiss_welcome(dont_show=True)
        app.until(lambda: json.loads((root_d / "Data" / "config.json").read_text()).get("showWelcome") is False, msg="welcome off")
        check("“Don't show this again” is stored in config.json", True)
        app.visible(".wizard")
        check("short first-run wizard (3 steps)", app.count(".wizard-steps li") == 3)
        check("archive id file created on first run", (root_d / ".gamecurator-id").is_file())
        check("database created in Data/library.db", (root_d / "Data" / "library.db").is_file())
        check("WebView profile stored on the archive drive", (root_d / "Data" / "WebView").is_dir())
        app.shot("01-wizard")
        for _ in range(2):
            app.click("#wz-next")
        app.click_text("Finish")
        app.gone(".wizard")
        cfg = json.loads((root_d / "Data" / "config.json").read_text())
        check("wizard completion stored in Data/config.json", cfg.get("wizardCompleted") is True)
        check("empty library shows a welcome panel", "archive is empty" in app.text("#lib-scroll"))
        app.shot("02-empty-library")

        # ---- Test 3: add game with artwork, edit, tag
        app.click("#lib-add")
        app.visible("#gf-title")
        pick(root_d / "Games" / "System Shock 2")
        app.click("#gf-browse")
        app.until(lambda: app.css("#gf-folder").get_attribute("value").endswith("System Shock 2"))
        check("folder picked and title suggested from folder name",
              app.css("#gf-title").get_attribute("value") == "System Shock 2")
        pick(WORK / "cover_ss2.png")
        app.click("#gf-art")
        app.until(lambda: app.count("#gf-cover img") == 1, msg="artwork preview")
        app.until(lambda: app.js("const i=document.querySelector('#gf-cover img');return i&&i.complete&&i.naturalWidth>0"), msg="artwork loaded")
        check("artwork uploaded and preview served from archive", True)
        app.until(lambda: app.count("#gf-cover .crop-frame.ready") == 1, msg="crop editor")
        check("artwork editor: 3:4 frame with zoom slider, Fit and Fill",
              app.count("#gf-cover .crop-zoom") == 1 and app.count("#gf-cover [data-fit]") == 1
              and abs(app.js("const r=document.querySelector('#gf-cover .crop-frame').getBoundingClientRect(); return r.height/r.width") - 4 / 3) < 0.02)
        # drag the wide picture to the right -> the card shows its left (blue) part
        app.js("""const f=document.querySelector('#gf-cover .crop-frame'); const r=f.getBoundingClientRect();
                  const ev=(t,x)=>f.dispatchEvent(new PointerEvent(t,{bubbles:true,button:0,pointerId:7,clientX:x,clientY:r.top+50}));
                  ev('pointerdown', r.left+20); ev('pointermove', r.left+120); ev('pointermove', r.left+220); ev('pointerup', r.left+220);""")
        app.type("#gf-dev", "Irrational Games")
        app.type("#gf-pub", "Electronic Arts")
        app.type("#gf-year", "1999")
        app.type("#gf-genre", "RPG")
        app.type("#gf-tags", "classic, immersive sim")
        app.type("#gf-desc", "Cyberpunk horror on the Von Braun.")
        app.shot("03-add-game-form")
        app.click("#gf-save")
        app.gone("#gf-title")
        app.until(lambda: app.count(".card") == 1, msg="card in grid")
        webps = list((root_d / "Data" / "Thumbnails").glob("*.webp"))
        check("optimized WebP thumbnail created", len(webps) == 1 and webps[0].stat().st_size < 60_000,
              f"{[(w.name, w.stat().st_size) for w in webps]}")
        crop = json.loads(sqlite3.connect(root_d / "Data" / "library.db").execute("select cover_crop from games").fetchone()[0] or "null")
        check("chosen part of the artwork stored (dragged to the left edge)", crop and abs(crop["x"]) < 0.001 and 0.4 < crop["w"] < 0.5, str(crop))
        check("thumbnail shows exactly the chosen part (left half only)",
              thumb_colour(webps[0], 20, 240) == "left" and thumb_colour(webps[0], 340, 240) == "left")
        app.until(lambda: app.js("const i=document.querySelector('.card img');return i&&i.complete&&i.naturalWidth>0"), msg="card image")
        check("card shows title, year and artwork", "System Shock 2" in app.text(".card") and "1999" in app.text(".card"), app.text(".card"))

        # ---- Test 4: archive content
        app.click(".card")
        app.visible(".profile-grid")
        app.until(lambda: app.count("#pf-content table.list tbody tr") >= 6, msg="file list")
        content_text = app.text("#pf-content")
        check("profile shows game, archive, notes and journal", app.count(".profile-grid > section") == 4)
        check("notes panel is called 'What I know about this game'", "WHAT I KNOW ABOUT THIS GAME" in app.text(".pf-d .strip").upper())
        groups = app.js("return [...document.querySelectorAll('#pf-content tr.group th')].map(t => t.innerText.split(' ')[0].toUpperCase())")
        check("archive files grouped by kind (installers, data, manuals, music, images, archives)",
              groups[:3] == ["INSTALLERS", "INSTALLER", "MANUALS"] and "MUSIC" in [g.split()[0] for g in groups] and "ARCHIVES" in groups, str(groups))
        check("archive summary shows file count and kinds", "6 files" in app.text(".archive-summary") and "1 manual" in app.text(".archive-summary"), app.text(".archive-summary"))
        no_tracking = "return document.querySelectorAll('#pf-status, #pf-rating, #pf-fav, .star-input, .stars, .badge.playing, .badge.completed, .badge.not_played, .badge.on_hold').length"
        check("no play status, rating or favourite controls in the profile", app.js(no_tracking) == 0)
        check("archive content lists installers, manual and soundtrack",
              all(s in content_text for s in ["setup_system_shock_2_1.0.2.exe", "Manual.pdf", "Soundtrack/01 - Main Theme.mp3"]))
        check("obvious installer pre-marked as primary",
              app.js("return [...document.querySelectorAll('#pf-content tbody tr:not(.group)')][0].innerText.includes('setup_system_shock_2_1.0.2.exe') && document.querySelector('#pf-content .primary-btn.on').closest('tr').innerText.includes('setup_system_shock_2_1.0.2.exe')"))
        app.shot("04-profile")
        # adjust the artwork later from the profile: show the whole picture
        app.js("document.getElementById('pf-adjust').click()")
        app.until(lambda: app.count(".overlay .crop-frame.ready") == 1, msg="adjust window")
        app.shot("04b-adjust-artwork")
        app.click(".overlay [data-fit]")
        app.click("#adj-save")
        app.gone(".overlay .crop-frame")
        app.until(lambda: "Artwork adjusted" in app.text("#sb-msg"), msg="adjusted")
        thumbs = list((root_d / "Data" / "Thumbnails").glob("*.webp"))
        crop = json.loads(sqlite3.connect(root_d / "Data" / "library.db").execute("select cover_crop from games").fetchone()[0])
        check("artwork adjusted in the profile: whole picture fits, old thumbnail replaced",
              len(thumbs) == 1 and thumbs[0] != webps[0] and crop["h"] > 2 and abs(crop["w"] - 1) < 0.01, f"{crop} {thumbs}")
        check("fitted thumbnail: dark bands, both halves of the picture",
              thumb_colour(thumbs[0], 180, 20) == "dark" and thumb_colour(thumbs[0], 60, 240) == "left" and thumb_colour(thumbs[0], 300, 240) == "right")
        # ---- manuals and music open in Game Curator's own windows
        LOG.write_text("")
        main_win = app.d.current_window_handle
        open_row = "[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes(arguments[0])).querySelector('[data-open]').click()"
        check("Open buttons say where PDFs and music open",
              app.js("return [...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('Manual.pdf')).querySelector('[data-open]').title")
              == "Read in the Game Curator viewer")
        t0 = time.perf_counter()
        app.js(open_row, "Manual.pdf")
        viewer_win = app.new_window([main_win])
        app.until(lambda: app.text("#vw-pages") == "3", msg="pdf loaded")
        app.until(lambda: app.count(".vw-page.drawn") >= 1, msg="page drawn")
        METRICS["pdf_first_page_s"] = round(time.perf_counter() - t0, 2)
        check("Open on a PDF manual opens it in the viewer window - no other program started",
              app.d.current_url.endswith("viewer.html") and not any(a.startswith("OPEN") for a in actions()), app.d.current_url)
        check("viewer window titled with the document and the game", app.d.title == "Manual \u2014 System Shock 2", app.d.title)
        ink = app.js("""const c=document.querySelector('.vw-page[data-page="1"] canvas'), d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;
                        let dark=0, n=0; for (let i=0;i<d.length;i+=16) { n++; if (d[i]<80 && d[i+1]<80 && d[i+2]<80) dark++; } return [dark, n];""")
        check("page 1 drawn with its text and picture (pdf.js, standard fonts from the app)", ink[0] > ink[1] * 0.02 and ink[0] < ink[1] * 0.5, str(ink))
        check("viewer: page counter and fit-width zoom",
              "3 pages" in app.text("#vw-status") and app.js("return document.getElementById('vw-zoom').value") == "width"
              and abs(app.js("return document.querySelector('.vw-page').offsetWidth") - (app.js("return document.getElementById('vw-scroll').clientWidth") - 32)) <= 2)
        app.shot("05c-pdf-viewer")
        app.click("#vw-next")
        app.until(lambda: app.js("return document.getElementById('vw-page').value") == "2", msg="next page")
        app.js("const s=document.getElementById('vw-zoom'); s.value='2'; s.dispatchEvent(new Event('change'))")
        app.until(lambda: app.js("return document.querySelector('.vw-page[data-page=\"1\"]').offsetWidth") == 1632, msg="zoom 200")
        app.type("#vw-page", "3" + Keys.ENTER)
        app.until(lambda: app.js("return document.getElementById('vw-page').value") == "3" and app.count(".vw-page[data-page='3'].drawn") == 1, msg="page 3")
        app.until(lambda: app.count(".vw-page[data-page='2'].drawn") == 1, msg="page 2 drawn")
        check("next page, page number and 200 % zoom (pages at twice their printed size; the A4 page keeps its own size)",
              app.js("return document.querySelector('.vw-page[data-page=\"2\"]').offsetWidth") == 1587
              and app.js("return document.getElementById('vw-scroll').scrollTop") > 1000)
        app.click("#vw-external")
        app.until(lambda: any(a.startswith("OPEN") for a in actions()), msg="external")
        check("viewer: 'Open in default app' hands the PDF to Windows", any(a.startswith("OPEN") and a.endswith("Manual.pdf") for a in actions()), str(actions()))
        app.d.switch_to.window(main_win)
        app.js(open_row, "Manual.pdf")
        time.sleep(1.0)
        check("the same manual again: its viewer comes to the front, no second window", len(app.d.window_handles) == 2)
        wm_close("^Manual")
        app.until(lambda: len(app.d.window_handles) == 1, msg="viewer closed")
        app.d.switch_to.window(main_win)
        LOG.write_text("")
        mp3 = root_d / "Games" / "System Shock 2" / "Soundtrack" / "01 - Main Theme.mp3"
        flac = root_d / "Games" / "System Shock 2" / "Soundtrack" / "02 - Combat.flac"
        app.js(open_row, "01 - Main Theme.mp3")
        player_win = app.new_window([main_win])
        audio_bytes = "return document.getElementById('pl-audio').dataset.bytes"
        app.until(lambda: app.js(audio_bytes) == str(mp3.stat().st_size), msg="track loaded")
        tracks = app.js("return [...document.querySelectorAll('.pl-track .pl-name')].map(e => e.innerText)")
        check("Open on a music file opens the music player with the game's soundtrack, starting with that track",
              app.d.current_url.endswith("player.html") and tracks == ["01 - Main Theme", "02 - Combat"]
              and app.text(".pl-track.on .pl-name") == "01 - Main Theme" and app.js("return document.getElementById('pl-game').textContent") == "System Shock 2"
              and not any(a.startswith("OPEN") for a in actions()), f"{tracks} {actions()}")
        app.until(lambda: app.js("const a=document.getElementById('pl-audio'); return a.duration > 3.5 && a.duration < 4.5"), msg="decoded")
        app.until(lambda: app.js("return document.getElementById('pl-audio').currentTime") > 0.2, msg="playing")
        check("the track plays (decoded: 0:04) with play/pause, previous, next, seek, volume and repeat",
              app.text("#pl-dur") == "0:04" and all(app.count(sel) == 1 for sel in ["#pl-play", "#pl-prev", "#pl-next", "#pl-seek", "#pl-vol", "#pl-repeat"]))
        app.shot("05d-music-player")
        app.click("#pl-play")
        app.until(lambda: app.js("return document.getElementById('pl-audio').paused"), msg="paused")
        app.click("#pl-next")
        app.until(lambda: app.js(audio_bytes) == str(flac.stat().st_size), msg="next track")
        check("pause, then next track (FLAC)", app.text("#pl-title") == "02 - Combat" and app.text("#pl-pos") == "2 / 2")
        app.click("#pl-repeat")
        check("repeat: all tracks", app.css("#pl-repeat").get_attribute("title") == "Repeat: all tracks")
        app.click("#pl-repeat")
        app.click("#pl-repeat")
        app.click("#pl-external")
        app.until(lambda: any(a.startswith("OPEN") for a in actions()), msg="external track")
        check("player: 'Open in default app' hands the track to Windows", any(a.endswith("02 - Combat.flac") for a in actions()), str(actions()))
        app.d.switch_to.window(main_win)
        app.js(open_row, "01 - Main Theme.mp3")
        app.d.switch_to.window(player_win)
        app.until(lambda: app.js(audio_bytes) == str(mp3.stat().st_size), msg="back to track 1")
        check("opening another track from the game: the same player switches to it", len(app.d.window_handles) == 2 and app.text("#pl-title") == "01 - Main Theme")
        wm_close("Music$")
        app.until(lambda: len(app.d.window_handles) == 1, msg="player closed")
        app.d.switch_to.window(main_win)
        app.js("document.querySelector('[data-dir=\"Soundtrack\"]').click()")
        app.until(lambda: any(a.endswith("Soundtrack") for a in actions()), msg="open folder")
        check("Open soundtrack subfolder in file manager", any(a.startswith("OPEN") and a.endswith("Soundtrack") for a in actions()))
        app.js("document.querySelector('#pf-content [data-run]').click()")
        app.visible(".overlay [data-ok]")
        check("running an EXE asks for confirmation first", "Run program?" in app.text(".overlay .window-title") and not any(a.startswith("RUN") for a in actions()))
        app.shot("05-run-confirmation")
        app.click(".overlay [data-ok]")
        app.until(lambda: any(a.startswith("RUN") for a in actions()), msg="run")
        check("installer runs only after confirmation", any(a.startswith("RUN") and a.endswith("setup_system_shock_2_1.0.2.exe") for a in actions()))
        # custom category
        app.js("const s=[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('artbook.zip')).querySelector('select'); s.value='image'; s.dispatchEvent(new Event('change',{bubbles:true}));")
        time.sleep(0.6)
        cat = sqlite3.connect(root_d / "Data" / "library.db").execute("select custom_category from files where filename='artbook.zip'").fetchone()
        check("custom file category stored", cat and cat[0] == "image", str(cat))
        # a name in the catalog (pencil next to Open); the file on disk keeps its name
        row_js = "return [...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes(arguments[0]))"
        app.js(row_js.replace("return ", "") + ".querySelector('[data-rename]').click()", "Manual.pdf")
        app.visible("#rn-name")
        check("rename window: only the name in the catalog, the file keeps its name",
              app.js("return document.getElementById('rn-name').value") == "Manual.pdf" and "keeps its own name" in app.text(".overlay"))
        app.type("#rn-name", "Original manual (English)")
        app.click("#rn-save")
        app.gone("#rn-name")
        app.until(lambda: "Original manual (English)" in app.text("#pf-content"), msg="renamed")
        manual = root_d / "Games" / "System Shock 2" / "Manual.pdf"
        dn = sqlite3.connect(root_d / "Data" / "library.db").execute("select display_name, filename from files where filename='Manual.pdf'").fetchone()
        tip = app.js("return [...document.querySelectorAll('#pf-content td.fname')].find(t=>t.innerText.includes('Original manual')).title")
        check("archive file shown under its new name; on disk still Manual.pdf (tooltip says so)",
              manual.is_file() and dn == ("Original manual (English)", "Manual.pdf") and "Manual.pdf" in tip
              and "Manual.pdf" not in app.text("#pf-content tbody"), f"{dn} {tip!r}")
        app.shot("05b-file-renamed")
        app.js(row_js.replace("return ", "") + ".querySelector('[data-rename]').click()", "Original manual")
        app.visible("#rn-reset")
        app.click("#rn-reset")
        app.gone("#rn-name")
        app.until(lambda: "Manual.pdf" in app.text("#pf-content tbody"), msg="name reset")
        check("'Use the file name' brings the file name back",
              sqlite3.connect(root_d / "Data" / "library.db").execute("select display_name from files where filename='Manual.pdf'").fetchone()[0] is None)

        # edit + tag + notes
        app.click("#pf-edit")
        app.visible("#gf-desc")
        app.type("#gf-desc", "Edited: the Many are waiting.")
        app.type("#gf-tags", "classic, immersive sim, favourite")
        app.click("#gf-save")
        app.gone("#gf-desc")
        app.until(lambda: "Edited: the Many" in app.text(".desc"), msg="desc updated")
        check("description edited", True)
        check("tag added, existing tags kept", all(t in app.text("#pf-info") for t in ["favourite", "classic", "immersive sim"]), app.text("#pf-info"))
        notes = app.css("#pf-notes")
        notes.send_keys("GOG offline installer v2.48, bought 2014.")
        app.click("#pf-notes-save")
        app.until(lambda: "saved" in app.text("#pf-notes-state").lower())
        check("'What I know about this game' saved", True)

        # ---- Test 5 (part 1): journal entry
        app.click("#pf-new-entry")
        app.visible(".editor")
        app.type("#je-title", "Medical deck")
        ed = app.css(".editor")
        ed.click()
        app.click(".editor-toolbar [title='Heading']")
        ed.send_keys("First session")
        ed.send_keys(Keys.ENTER)
        app.click(".editor-toolbar [title='Paragraph']")
        ed.send_keys("Found the wrench. Shodan is watching.")
        ed.send_keys(Keys.ENTER)
        app.click(".editor-toolbar [title='Bulleted list']")
        ed.send_keys("Keep psi hypos")
        app.shot("06-journal-editor")
        app.click("#je-save")
        app.gone(".editor")
        app.until(lambda: "Medical deck" in app.text("#pf-journal"))
        row = sqlite3.connect(root_d / "Data" / "library.db").execute("select content, entry_date from journal_entries").fetchone()
        check("journal entry stored with heading, paragraph and list", row and "<h2>" in row[0] and "<ul>" in row[0] and "<li>" in row[0], str(row))
        check("entry date defaults to today", row and row[1] == time.strftime("%Y-%m-%d"), str(row))
        app.click("[data-edit-entry]")
        app.visible(".editor")
        app.type("#je-title", "Medical deck (edited)")
        app.click("#je-save")
        app.gone(".editor")
        app.until(lambda: "Medical deck (edited)" in app.text("#pf-journal"))
        check("journal entry edited", True)
        mirrors = list((root_d / "Data" / "Journals").rglob("*.html"))
        check("readable HTML copy written to Data/Journals", len(mirrors) == 1 and "Medical deck (edited)" in mirrors[0].read_text())
        app.shot("07-profile-with-journal")

        # ---- Test 6: scanner
        app.click("#nav-scanner")
        app.type("#sc-root", str(gog))
        app.click("#sc-start")
        app.until(lambda: app.count("#sc-results tbody tr") >= 4, timeout=30, msg="scan results")
        rows_text = app.text("#sc-results")
        check("scanner finds all 4 games and skips the docs folder",
              app.count("#sc-results tbody tr") == 4 and "Docs" not in app.js("return [...document.querySelectorAll('#sc-results .cand-title')].map(i=>i.value).join('|')"),
              app.js("return [...document.querySelectorAll('#sc-results .cand-title')].map(i=>i.value).join('|')"))
        titles = app.js("return [...document.querySelectorAll('#sc-results .cand-title')].map(i=>i.value)")
        check("candidate titles cleaned (no [GOG], no year brackets)", sorted(titles) == sorted(["Baldurs Gate 3", "Disco Elysium", "Gothic", "Heroes of Might and Magic 3"]), str(titles))
        app.shot("08-scanner-results")
        # choose only some: unselect Heroes
        app.js("const r=[...document.querySelectorAll('#sc-results tbody tr')].find(r=>r.querySelector('.cand-title').value.startsWith('Heroes')); r.querySelector('[data-sel]').click();")
        app.js("const r=[...document.querySelectorAll('#sc-results tbody tr')].find(r=>r.querySelector('.cand-title').value.startsWith('Disco')); const i=r.querySelector('[data-year]'); i.value='2019'; i.dispatchEvent(new Event('input',{bubbles:true}));")
        pick(WORK / "cover_de.jpg")
        app.js("const r=[...document.querySelectorAll('#sc-results tbody tr')].find(r=>r.querySelector('.cand-title').value.startsWith('Disco')); r.querySelector('[data-art]').click();")
        app.until(lambda: app.count("#sc-results .thumb-mini") == 1, msg="artwork in scanner")
        app.click("#sc-import")
        app.visible(".overlay [data-ok]")
        check("import report shows 3 imported", "3 game(s) imported" in app.text(".overlay .dlg-text"), app.text(".overlay .dlg-text"))
        app.click(".overlay [data-ok]")
        app.click("#sc-start")
        time.sleep(1.0)
        app.until(lambda: app.count("#sc-results tbody tr") == 4 and "In catalog" in app.text("#sc-results"), timeout=30)
        in_cat = app.js("return [...document.querySelectorAll('#sc-results tbody tr')].filter(r=>r.innerText.includes('In catalog')).length")
        check("re-scan recognises imported games", in_cat == 3, str(in_cat))
        check("already catalogued candidates cannot be selected again",
              app.js("return [...document.querySelectorAll('#sc-results tbody tr')].filter(r=>r.innerText.includes('In catalog')).every(r=>r.querySelector('[data-sel]').disabled)"))
        n = sqlite3.connect(root_d / "Data" / "library.db").execute("select count(*), count(distinct folder_key) from games").fetchone()
        check("no duplicates in the database", n[0] == 4 and n[1] == 4, str(n))
        # cancel a running scan: big tree (40 000 folders, tmpfs) and the
        # Cancel button is pressed as soon as the first progress event arrives
        many = Path("/dev/shm/gc-e2e-many")
        if not many.exists():
            for g in range(40):
                for i in range(1000):
                    (many / f"Group {g}" / f"Folder {i}").mkdir(parents=True, exist_ok=True)
        app.type("#sc-root", str(many))
        app.js("""window.__TAURI__.event.listen('scan-progress', () => { const c = document.getElementById('sc-cancel'); if (!c.disabled) c.click(); });
                  document.getElementById('sc-start').click();""")
        app.until(lambda: not app.css("#sc-start").get_attribute("disabled") and "cancel" in app.text("#sc-results").lower(), timeout=60)
        check("a running scan can be cancelled", "cancelled" in app.text("#sc-results").lower(), app.text("#sc-results")[:200])
        app.shot("08b-scan-cancelled")

        # ---- library: search and sort only, no filters
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 4)
        app.type("#lib-search", "disco")
        app.until(lambda: app.count(".card") == 1)
        check("search filters the library", "Disco Elysium" in app.text(".card"))
        check("search shows 'x of total'", "1 of 4" in app.text("#lib-count"), app.text("#lib-count"))
        app.css("#lib-search").clear()
        app.css("#lib-search").send_keys(" ")
        app.css("#lib-search").send_keys(Keys.BACKSPACE)
        app.until(lambda: app.count(".card") == 4)
        check("no filters anywhere (status, genre, tag, availability)",
              app.count("[data-filter], [data-tag], #sidebar .side-list, .filters") == 0
              and not any(w in app.text("body").lower() for w in ["not played", "completed", "on hold", "favorite"]),
              app.text(".sidebar"))
        check("cards show year and size on disk, no status", "1999 · 22 KB" in app.text(".card:last-child") or "1999" in app.text(".card:last-child") and "KB" in app.text(".card:last-child"), app.text(".card:last-child"))
        app.js("const s=document.getElementById('lib-sort'); s.value='year'; s.dispatchEvent(new Event('change'))")
        app.until(lambda: "Disco" in app.text(".card:first-child"), msg="sort by year")
        check("sort by release year (newest first, unknown years last)", "Disco" in app.text(".card:first-child") and "System Shock" in app.text(".card:nth-child(2)"), app.text(".card:first-child"))
        app.js("const s=document.getElementById('lib-sort'); s.value='title'; s.dispatchEvent(new Event('change'))")
        app.until(lambda: "Baldurs" in app.text(".card:first-child"), msg="sort by title")
        app.shot("09-library-grid")
        app.click("[data-mode='catalog']")
        app.until(lambda: app.count("table.list tbody tr") == 4)
        heads = app.text("table.list thead").upper()
        check("Catalog View lists title, year, genre, files, size, folder (no status)",
              all(h in heads for h in ["TITLE", "YEAR", "GENRE", "FILES", "SIZE", "FOLDER"]) and "STATUS" not in heads, heads)
        app.shot("10-library-catalog")
        app.click("[data-mode='grid']")

        # ---- installed games on this PC
        app.click("#nav-settings")
        app.click("#st-detect")
        app.until(lambda: "installed on E2E-PC" in app.text("#st-detect-result"), msg="detect")
        check("installed games looked up on request (GOG record with an edition name matched)",
              "1 found automatically" in app.text("#st-detect-result"), app.text("#st-detect-result"))
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 4)
        on = app.js("return [...document.querySelectorAll('.card')].filter(c=>c.querySelector('.inst-box.on')).map(c=>c.querySelector('.t').innerText)")
        check("library: checkbox icon on every card, ticked for the installed game",
              app.count(".card .inst-box") == 4 and on == ["Disco Elysium"], str(on))
        tcols = app.js("return [...document.querySelectorAll('.card .t')].map(t => [t.innerText, getComputedStyle(t).color])")
        check("Classic: titles of games not installed in orange, the installed one in green",
              all(c == ("rgb(143, 212, 154)" if "Disco" in t else "rgb(244, 163, 90)") for t, c in tcols), str(tcols))
        app.click("[data-mode='catalog']")
        app.until(lambda: app.count("table.list tbody tr") == 4)
        check("catalog: 'installed' column", app.count("table.list tbody .inst-box.on") == 1 and app.count("table.list thead .inst-col") == 1)
        app.until(lambda: app.js("const i=document.querySelector('table.list img.game-icon'); return !!i && i.complete && i.naturalWidth > 0"), msg="icon")
        icon_row = app.js("return document.querySelector('table.list img.game-icon').closest('tr').innerText")
        cols = app.js("return [...document.querySelectorAll('table.list tr')].map(r => r.children[1].getBoundingClientRect().left)")
        order = app.js("return [...document.querySelector('table.list thead tr').children].map(c => c.className.split(' ')[0] || c.innerText)")
        icon_rel = sqlite3.connect(root_d / "Data" / "library.db").execute("select icon_path from games where title like 'Disco%'").fetchone()[0]
        check("catalog: icon of the installed game between the installed column and Title, aligned",
              app.count("table.list img.game-icon") == 1 and "Disco Elysium" in icon_row and [o.upper() for o in order[:3]] == ["INST-COL", "ICO-COL", "TITLE"]
              and len(set(round(x) for x in cols)) == 1, f"{icon_row!r} {order} {cols}")
        check("…read from GOG's goggame-*.ico (cached in Data/Thumbnails), the installation untouched",
              icon_rel and icon_rel.startswith("Data/Thumbnails/icon-") and (root_d / icon_rel).is_file()
              and (WORK / "C" / "GOG Games" / "Disco Elysium" / "goggame-1771589310.ico").read_bytes() == de_ico_bytes, str(icon_rel))
        app.shot("10b-catalog-icons")
        app.click("[data-mode='grid']")
        app.until(lambda: app.count(".card") == 4)
        LOG.write_text("")
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('Disco')).click()")
        app.visible("#pf-install")
        inst_tip = app.js("return document.querySelector('#pf-install .inst-text').title")
        check("profile: installed; the path is not printed (tooltip and 'Show location' tell where)",
              "Installed on this PC" in app.text("#pf-install") and "GOG Games" not in app.text("#pf-install")
              and "GOG Games" in inst_tip and app.count("#pf-inst-folder") == 1, f"{app.text('#pf-install')!r} / {inst_tip!r}")
        green = app.js("return getComputedStyle(document.querySelector('.profile-head h1')).color")
        check("Classic: title of an installed game in green", green == "rgb(143, 212, 154)", green)
        check("Play sits in the 'Installed on this PC' box (not in the header)", app.count("#pf-install #pf-play") == 1 and app.count(".profile-head #pf-play") == 0)
        app.click("#pf-play")
        app.visible(".overlay [data-ok]")
        check("Play asks for confirmation first", "Start game?" in app.text(".overlay .window-title") and not any(a.startswith("PLAY") for a in actions()))
        app.shot("12c-play-confirmation")
        app.click(".overlay [data-ok]")
        app.until(lambda: any(a.startswith("PLAY") for a in actions()), msg="play")
        check("Play starts the program from goggame-*.info", any(a.startswith("PLAY") and a.endswith("disco.exe") for a in actions()), str(actions()))
        # ---- after playing: the end of the game is noticed, a journal entry offered
        app.until(lambda: app.js("const b=document.getElementById('pf-play'); return !!b && b.disabled && b.innerText.includes('Running')"), msg="running")
        check("while the game runs, Play shows 'Running' (no second start)", True)
        main_win = app.d.current_window_handle
        ended_file = de_inst / "ended"
        offer_win = app.new_window([main_win], timeout=20)
        appeared = app.js("return performance.timeOrigin") / 1000
        app.until(lambda: "Disco Elysium" in app.text("#of-text"), msg="offer text")
        delay = appeared - float(ended_file.read_text())
        METRICS["journal_offer_after_game_end_s"] = round(delay, 2)
        check("the moment the game ends (launcher quit at once, the game 2 s later): the journal offer appears within 2 s, in a window of its own",
              app.d.current_url.endswith("offer.html") and -0.5 < delay < 2.0, f"{delay:.2f} s")
        # "always on top" itself cannot be seen here: without a window manager (Xvfb) no window is above
        # another - see docs/TESTING.md, section 4
        check("offer: the game, 'Don't show this again', 'Not now', 'Write entry'",
              "Back from the game" in app.d.title and "You just finished playing Disco Elysium" in app.text("#of-text")
              and app.count("#of-never") == 1 and app.text("#of-later") == "Not now" and "Write entry" in app.text("#of-write"))
        check("…without any play time (an archive, not a counter)", not re.search(r"\d+\s*(min|h|hour|minute)", app.text("body")))
        app.shot("12c2-journal-offer")
        app.click("#of-write")
        app.until(lambda: len(app.d.window_handles) == 1, msg="offer closed")
        app.d.switch_to.window(main_win)
        app.visible(".overlay .editor")
        check("'Write entry' closes the offer and opens a new journal entry for the game in the main window",
              "Disco Elysium" in app.text(".profile-head") and app.js("return document.getElementById('pf-play').innerText").strip().endswith("Play"))
        app.js("document.querySelector('.overlay .window-title .x').click()")
        app.gone(".overlay")
        # don't show again -> no more offers (until turned on in Settings)
        ended_file.unlink()
        app.click("#pf-play")
        app.click(".overlay [data-ok]")
        app.new_window([main_win], timeout=20)
        app.until(lambda: "Disco Elysium" in app.text("#of-text"), msg="second offer")
        app.click("#of-never")
        app.click("#of-later")
        app.until(lambda: len(app.d.window_handles) == 1, msg="offer closed")
        app.d.switch_to.window(main_win)
        app.until(lambda: json.loads((root_d / "Data" / "config.json").read_text()).get("journalPrompt") is False, msg="prompt off")
        check("'Don't show this again' turns the offer off (config.json)", True)
        app.click("#pf-play")
        app.click(".overlay [data-ok]")
        app.until(lambda: app.js("const b=document.getElementById('pf-play'); return !!b && !b.disabled"), timeout=20, msg="ended again")
        time.sleep(1.5)
        check("…and the next game end asks nothing", len(app.d.window_handles) == 1 and app.count(".overlay") == 0)
        app.click("#pf-inst-folder")
        app.until(lambda: any(a.startswith("OPEN") for a in actions()), msg="show location")
        check("'Show location' opens the installation folder", any(a.startswith("OPEN") and a.endswith("Disco Elysium") for a in actions()), str(actions()))
        app.shot("12d-profile-installed")

        # ---- divider between the game column and the archive
        cfg_path = root_d / "Data" / "config.json"
        w0 = app.js("return document.querySelector('.pf-a').getBoundingClientRect().width")
        check("profile divider present (separator, keyboard focusable)",
              app.count("#pf-split[role='separator'][tabindex='0']") == 1 and abs(json.loads(cfg_path.read_text())["profileSplit"] - 42.5) < 0.01)
        app.js("""const b=document.getElementById('pf-split'); const g=document.getElementById('pf-grid').getBoundingClientRect();
                  const ev=(t,x)=>b.dispatchEvent(new PointerEvent(t,{bubbles:true,button:0,pointerId:9,clientX:x,clientY:g.top+200}));
                  ev('pointerdown', g.left+g.width*0.43); ev('pointermove', g.left+g.width*0.55); ev('pointermove', g.left+g.width*0.62); ev('pointerup', g.left+g.width*0.62);""")
        app.until(lambda: abs(json.loads(cfg_path.read_text()).get("profileSplit", 0) - 62) < 3, msg="split saved")
        w1 = app.js("return document.querySelector('.pf-a').getBoundingClientRect().width")
        check("dragging the divider widens the game column and is saved", w1 > w0 + 150, f"{w0:.0f} -> {w1:.0f}, {json.loads(cfg_path.read_text())['profileSplit']}")
        app.shot("12e-profile-divider")
        app.click("#pf-back")
        app.until(lambda: app.count(".card") == 4)
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('Disco')).click()")
        app.visible("#pf-split")
        w2 = app.js("return document.querySelector('.pf-a').getBoundingClientRect().width")
        check("the chosen width stays for every profile", abs(w2 - w1) < 4, f"{w1:.0f} vs {w2:.0f}")
        app.css("#pf-split").send_keys(Keys.ARROW_LEFT)
        app.until(lambda: app.js("return document.querySelector('.pf-a').getBoundingClientRect().width") < w2 - 10, msg="arrow key")
        app.until(lambda: json.loads(cfg_path.read_text())["profileSplit"] < 61, msg="arrow key saved")
        check("arrow keys move the divider", True)
        app.js("document.getElementById('pf-split').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))")
        app.until(lambda: abs(json.loads(cfg_path.read_text())["profileSplit"] - 42.5) < 0.01, msg="reset")
        w3 = app.js("return document.querySelector('.pf-a').getBoundingClientRect().width")
        check("double-click resets the divider", abs(w3 - w0) < 4, f"{w0:.0f} vs {w3:.0f}")
        app.click("#pf-back")
        app.until(lambda: app.count(".card") == 4)
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('System Shock')).click()")
        app.visible("#pf-install")
        check("not detected: 'Not installed on this PC', no Play", "Not installed" in app.text("#pf-install") and app.count("#pf-play") == 0)
        pick(ss2_inst / "ss2.exe")
        app.click("#pf-inst-locate")
        app.until(lambda: app.count("#pf-play") == 1, msg="located")
        check("located by the user: Play available ('set by you' in the tooltip)",
              "set by you" in app.js("return document.querySelector('#pf-install .inst-text').title"), app.text("#pf-install"))
        app.click("#pf-back")
        app.until(lambda: app.count(".card") == 4 and app.count(".card .inst-box.on") == 2, msg="two installed")

        # ---- screenshots
        pics = WORK / "Pictures"
        db_path = root_d / "Data" / "library.db"
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('System Shock')).click()")
        app.visible("#pf-shots")
        check("Screenshots panel under the journal, with the prompt",
              "Discovered or experienced something interesting in the game? Capture it!" in app.text("#pf-shots")
              and app.js("const a=document.querySelector('.pf-c').getBoundingClientRect(), b=document.querySelector('.pf-e').getBoundingClientRect(); return b.top >= a.bottom - 1 && Math.abs(a.left-b.left) < 2"))
        pick_many([pics / f"shot{i}.png" for i in range(1, 5)])
        app.click("#pf-add-shot")
        app.until(lambda: app.count("#pf-shots .shot") == 3, msg="three previews")
        stored = list((root_d / "Data" / "Screenshots").rglob("shot-*.webp"))
        check("4 screenshots added: 3 newest previews, the rest in the gallery",
              len(stored) == 4 and "All 4 screenshots" in app.text("#pf-shots") and all((pics / f"shot{i}.png").is_file() for i in range(1, 5)),
              f"{len(stored)} {app.text('#pf-shots')}")
        app.click("#pf-shots .shot")
        app.until(lambda: app.js("const i=document.querySelector('.gal-pic img');return i&&i.complete&&i.naturalWidth===1920"), msg="large picture")
        check("a preview opens the large picture in the gallery", "1 / 4" in app.text(".gal-info"))
        app.type("#gal-caption", "Shodan on deck 5")
        app.css("#gal-caption").send_keys(Keys.ENTER)
        app.until(lambda: sqlite3.connect(db_path).execute("select count(*) from screenshots where caption='Shodan on deck 5'").fetchone()[0] == 1, msg="caption")
        check("caption saved", True)
        app.css("body").send_keys(Keys.ARROW_RIGHT)
        app.until(lambda: "2 / 4" in app.text(".gal-info"), msg="next")
        check("arrow keys move through the gallery", True)
        app.click("#gal-grid")
        app.until(lambda: app.count(".gal-item") == 4)
        check("the whole gallery as a grid, captions shown", "Shodan on deck 5" in app.text(".gal-grid"))
        app.shot("12f-gallery")
        app.click(".gal-item:last-child")
        app.click("#gal-delete")
        app.visible(".overlay [data-ok]")
        check("deleting a screenshot asks first", "Delete screenshot?" in app.text(".overlay:last-child .window-title"))
        app.click(".overlay:last-child [data-ok]")
        app.until(lambda: app.count(".gal-item") == 3 or "/ 3" in app.text(".gal-info"), msg="deleted")
        check("screenshot deleted (stored copy), the original picture stays",
              len(list((root_d / "Data" / "Screenshots").rglob("shot-*.webp"))) == 3 and (pics / "shot1.png").is_file())
        app.css("body").send_keys(Keys.ESCAPE)
        app.gone(".gallery-window")
        app.until(lambda: app.count("#pf-shots .shot") == 3 and "Open the gallery" in app.text("#pf-shots"), msg="strip refreshed")
        app.js("document.getElementById('pf-shots').scrollIntoView({block:'end'})")
        time.sleep(0.4)
        app.shot("12h-profile-screenshots")
        # drag and drop from Explorer
        app.drop([pics / "shot1.png"], ".pf-e")
        app.until(lambda: "All 4 screenshots" in app.text("#pf-shots"), msg="dropped picture")
        check("a picture dragged onto Screenshots is added", len(list((root_d / "Data" / "Screenshots").rglob("shot-*.webp"))) == 4)
        app.drop([pics], ".pf-e")
        app.until(lambda: "Only files can be dropped" in app.text("#sb-msg") or "Only files" in app.text("body"), msg="folder refused")
        check("a dropped folder is refused (files only)", len(list((root_d / "Data" / "Screenshots").rglob("shot-*.webp"))) == 4)
        app.drop([pics / "shot2.png"], ".pf-a")
        app.until(lambda: "Drop files on" in app.text("body"), msg="no zone")
        check("dropped outside a drop area: nothing added, a hint shown", len(list((root_d / "Data" / "Screenshots").rglob("shot-*.webp"))) == 4)
        # a screenshot that belongs to a journal entry
        app.click("#pf-new-entry")
        app.visible(".editor")
        app.type("#je-title", "The Rickenbacker")
        pick_many([pics / "shot5.png"])
        app.click("#je-add-shot")
        app.until(lambda: app.count(".je-shots .shot") == 1, msg="attached in editor")
        app.click("#je-save")
        app.gone(".editor")
        app.until(lambda: app.count("#pf-journal .entry-shots .shot") == 1, msg="entry shows screenshot")
        row = sqlite3.connect(db_path).execute("select s.journal_entry_id, j.title from screenshots s join journal_entries j on j.id = s.journal_entry_id").fetchall()
        mirror = [m.read_text() for m in (root_d / "Data" / "Journals").rglob("*.html") if "Rickenbacker" in m.read_text()]
        check("screenshot added in the journal editor belongs to the entry (also in its HTML copy)",
              row and row[0][1] == "The Rickenbacker" and mirror and "../../Screenshots/" in mirror[0], f"{row}")
        check("…and is part of the game's screenshots too", "All 5 screenshots" in app.text("#pf-shots"), app.text("#pf-shots"))
        app.js("document.querySelector('#pf-journal .entry-shots .shot').click()")
        app.until(lambda: app.count(".gal-info") == 1, msg="gallery from entry")
        check("from the entry the gallery shows where the screenshot belongs", "The Rickenbacker" in app.text(".gal-info"), app.text(".gal-info"))
        app.css("body").send_keys(Keys.ESCAPE)
        app.gone(".gallery-window")

        # ---- additional files
        check("Additional files panel under Archive, with the explanation",
              "Compatibility fixes, community patches" in app.text("#pf-additions")
              and app.js("const a=document.querySelector('.pf-b').getBoundingClientRect(), b=document.querySelector('.pf-f').getBoundingClientRect(); return b.top >= a.bottom - 1 && Math.abs(a.left-b.left) < 2"))
        LOG.write_text("")
        pick(dl / "widescreen_fix_1.3.zip")
        app.click("#pf-add-file")
        app.visible("#ad-save")
        check("a file outside the archive is copied by default", app.js("return document.querySelector('input[name=ad-mode]:checked').value") == "copy")
        app.js("document.getElementById('ad-kind').value='fix'")
        app.type("#ad-note", "Needed for 16:9 monitors")
        app.click("#ad-save")
        app.gone("#ad-save")
        copy = root_d / "Data" / "Additions" / "game-1" / "widescreen_fix_1.3.zip"
        app.until(lambda: app.count("table.additions tr") == 1, msg="copied file listed")
        check("copied into the archive (Data/Additions), the original stays",
              copy.is_file() and copy.read_bytes() == (dl / "widescreen_fix_1.3.zip").read_bytes() and (dl / "widescreen_fix_1.3.zip").is_file()
              and "Needed for 16:9 monitors" in app.text("table.additions"))
        pick(dl / "Making of System Shock 2.mkv")
        app.click("#pf-add-file")
        app.visible("#ad-save")
        app.js("document.querySelector('input[name=ad-mode][value=link]').click(); document.getElementById('ad-kind').value='bonus'")
        app.click("#ad-save")
        app.gone("#ad-save")
        app.until(lambda: app.count("table.additions tr") == 2, msg="linked file listed")
        check("a big file can be linked where it is (nothing copied)",
              not (root_d / "Data" / "Additions" / "game-1" / "Making of System Shock 2.mkv").exists() and "linked" in app.text("table.additions"))
        pick(dl / "ss2_community_patch.exe")
        app.click("#pf-add-file")
        app.visible("#ad-save")
        app.js("document.querySelector('input[name=ad-mode][value=link]').click(); document.getElementById('ad-kind').value='patch'")
        app.click("#ad-save")
        app.gone("#ad-save")
        app.until(lambda: app.count("table.additions tr") == 3)
        kinds = app.js("return [...document.querySelectorAll('table.additions .add-kind')].map(e=>e.innerText.trim().toLowerCase())")
        check("sorted by kind: fix, patch, bonus", kinds == ["fix", "patch", "bonus"], str(kinds))
        app.click("table.additions tr:nth-child(2) [data-add-edit]")
        app.visible("#ae-name")
        app.type("#ae-name", "Community patch (SS2Tool)")
        app.click("#ae-save")
        app.gone("#ae-name")
        app.until(lambda: "Community patch (SS2Tool)" in app.text("table.additions"), msg="addition renamed")
        check("additional file renamed in the catalog (pencil next to Open); the file keeps its name",
              (dl / "ss2_community_patch.exe").is_file() and "ss2_community_patch.exe" not in app.text("table.additions")
              and sqlite3.connect(root_d / "Data" / "library.db").execute(
                  "select display_name, filename from additions where filename='ss2_community_patch.exe'").fetchone()
              == ("Community patch (SS2Tool)", "ss2_community_patch.exe"))
        app.shot("12g-additional-files")
        app.click("[data-add-run]")
        app.visible(".overlay [data-ok]")
        check("a patch program asks before running", "Run program?" in app.text(".overlay .window-title") and not any(a.startswith("RUN") for a in actions()))
        app.click(".overlay [data-ok]")
        app.until(lambda: any(a.startswith("RUN") for a in actions()), msg="run patch")
        check("…and runs after the confirmation", any(a.endswith("ss2_community_patch.exe") for a in actions()), str(actions()))
        app.click("table.additions tr:first-child [data-add-open]")
        app.until(lambda: any(a.startswith("OPEN") and a.endswith("widescreen_fix_1.3.zip") for a in actions()), msg="open copy")
        app.click("table.additions tr:last-child [data-add-show]")
        app.until(lambda: any(a.startswith("OPEN") and a.endswith("Downloads") for a in actions()), msg="show folder")
        check("Open and Folder work for copied and linked files", True)
        app.click("table.additions tr:first-child [data-add-remove]")
        app.visible("#ar-ok")
        app.click("#ar-ok")
        app.gone("#ar-ok")
        app.until(lambda: app.count("table.additions tr") == 2)
        check("taken off the list; the copy deleted on request, the original stays", not copy.exists() and (dl / "widescreen_fix_1.3.zip").is_file())
        app.click("table.additions tr:last-child [data-add-remove]")
        app.visible(".overlay [data-ok]")
        app.click(".overlay [data-ok]")
        app.until(lambda: app.count("table.additions tr") == 1)
        check("a linked file is never deleted", (dl / "Making of System Shock 2.mkv").is_file())
        app.js("""const r=document.querySelector('.pf-f').getBoundingClientRect();
                  window.__TAURI__.event.emit('gc-drag', {kind: 'over', x: r.left + r.width / 2, y: r.top + r.height / 2});""")
        app.until(lambda: app.count(".pf-f.drop-over") == 1 and app.count("html.dragging-files") == 1, msg="drag highlight")
        check("dragging files over a panel highlights it with a hint",
              "Drop files here" in app.js("return getComputedStyle(document.querySelector('.pf-f'), '::after').content"))
        app.shot("12i-drag-over")
        app.js("window.__TAURI__.event.emit('gc-drag', {kind: 'leave', x: 0, y: 0})")
        app.until(lambda: app.count(".drop-over") == 0, msg="highlight off")
        app.drop([dl / "widescreen_fix_1.3.zip", dl / "Making of System Shock 2.mkv"], ".pf-f")
        app.visible("#ad-save")
        check("two files dragged onto Additional files: one dialog for both", "Add 2 files to the game" in app.text(".overlay .window-title"))
        app.js("document.querySelector('input[name=ad-mode][value=link]').click(); document.getElementById('ad-kind').value='bonus'")
        app.click("#ad-save")
        app.gone("#ad-save")
        app.until(lambda: app.count("table.additions tr") == 3, msg="dropped files listed")
        check("…both added (linked, nothing copied)", not (root_d / "Data" / "Additions" / "game-1" / "Making of System Shock 2.mkv").exists())
        app.click("#pf-back")
        app.until(lambda: app.count(".card") == 4)

        # ---- left menu, journal overview, about
        check("no top bar", app.count(".topbar, #topbar") == 0)
        nav = app.js("return [...document.querySelectorAll('#nav button')].map(b => b.id + ':' + (b.querySelector('svg') ? 'svg' : '-') + ':' + b.innerText.trim())")
        check("left menu: Library, Gaming Journal, Archive Scanner, Settings, About - each with an icon",
              [n.split(":")[0] for n in nav] == ["nav-library", "nav-journal", "nav-scanner", "nav-settings", "nav-about"]
              and all(":svg:" in n for n in nav), str(nav))
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('Disco')).click()")
        app.visible("#pf-new-entry")
        app.click("#pf-new-entry")
        app.visible(".editor")
        app.type("#je-title", "Thought cabinet")
        app.css(".editor").click()
        app.css(".editor").send_keys("Inland Empire knows more than I do.")
        app.click("#je-save")
        app.gone(".editor")
        app.until(lambda: "Thought cabinet" in app.text("#pf-journal"))
        app.click("#nav-journal")
        app.until(lambda: app.count(".jgroup") == 2, msg="journal groups")
        groups = app.js("return [...document.querySelectorAll('.jgroup .jgroup-title')].map(e => e.innerText)")
        check("Gaming Journal page gathers entries of all games, grouped by game", groups == ["Disco Elysium", "System Shock 2"], str(groups))
        check("journal groups show entry count", "2 entries" in app.text(".jgroup:last-child .jgroup-head"), app.text(".jgroup:last-child .jgroup-head"))
        check("journal page shows the screenshots of an entry", app.count(".jgroup .entry-shots .shot") == 1)
        check("journal page has no filters and no 'new entry' (entries are written in the game profile)",
              app.count("#main select, #jr-new, #jr-game") == 0)
        check("journal overview header", "3 entries · 2 games" in app.text("#jr-count"), app.text("#jr-count"))
        app.shot("12a-journal-overview")
        app.click(".jgroup:last-child .jgroup-head")
        app.until(lambda: app.count("#pf-notes") == 1)
        check("game header in the journal opens the game profile", "System Shock 2" in app.text("#main") and app.count(".jgroup") == 0, app.text("#main")[:120])
        app.click("#nav-about")
        app.until(lambda: app.count(".about") == 1)
        about = app.text(".about")
        check("About states the main idea of the project",
              all(w in about for w in ["DRM-free", "at your own pace", "not a backlog", "Your games. Your archive. Your history.",
                                       "Gaming history deserves careful, loving treatment."]), about[:300])
        check("About shows version, archive location and archive ID", all(w in about for w in ["Version", "Archive ID", str(root_d.name)]), about[-300:])
        check("About menu item highlighted", "active" in app.css("#nav-about").get_attribute("class"))
        app.shot("12b-about")
        app.click("#nav-settings")
        app.until(lambda: app.count("#st-wizard") == 1)
        check("Settings no longer duplicate the About information", "Archive ID" not in app.text("#main"))
        app.click("#nav-library")

        # ---- remove game (folder must stay)
        app.until(lambda: app.count(".card") == 4)
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('Gothic')).click()")
        app.visible("#pf-remove")
        app.click("#pf-remove")
        app.visible("#rm-ok")
        check("removal dialog states the folder is not deleted", "NOT be deleted" in app.text(".overlay .dlg-text"))
        app.shot("11-remove-confirmation")
        app.click("#rm-ok")
        app.until(lambda: app.count(".card") == 3)
        check("game removed from catalog, original folder kept",
              (gog / "Gothic (2001)" / "setup_gothic_1.0.2.exe").is_file() and (gog / "Gothic (2001)" / "Manual.pdf").is_file())

        # ---- settings: backup
        app.click("#nav-settings")
        app.click("#st-backup")
        app.until(lambda: app.count("#st-backups tbody tr") >= 1)
        check("manual database backup created", any((root_d / "Data" / "Backups").glob("library-*-manual.db")))
        app.shot("12-settings")

        # ---- themes: appearance only
        check("four themes offered in Settings (incl. DOS Midnight)", app.count("input[name=theme]") == 4 and "DOS Midnight" in app.text("#st-theme"))
        check("classic theme is the default", app.js("return document.documentElement.dataset.theme") == "classic")
        check("classic: pixel font (Pixel Operator) loaded from the app, no system font fallback",
              "Pixel Operator" in app.js("return getComputedStyle(document.body).fontFamily")
              and app.js("return document.fonts.check('16px \"Pixel Operator\"') && document.fonts.check('bold 16px \"Pixel Operator 8\"')")
              and app.js("return [...document.fonts].filter(f => f.status === 'loaded' && /Pixel Operator/.test(f.family)).length") >= 2,
              app.js("return [...document.fonts].map(f => f.family + ':' + f.weight + ':' + f.status).join(', ')"))
        body_color = app.js("return getComputedStyle(document.body).color")
        check("classic: body text in a light grey, not white", body_color == "rgb(195, 191, 203)", body_color)
        # ---- font of the Classic look
        cfg_file = root_d / "Data" / "config.json"
        check("Settings offer 4 fonts for the Classic look, each sample in its own font",
              app.count("input[name=classicFont]") == 4 and app.js("return document.documentElement.dataset.classicFont") == "pixel-operator"
              and app.js("return getComputedStyle(document.querySelector('.font-sample.font-dogica')).fontFamily").startswith('"Dogica Pixel"'))
        font_info = {}
        for key, family, size in [("dogica", "Dogica Pixel", "8px"), ("geist-pixel", "Geist Pixel", "14px"), ("roboto-slab", "Roboto Slab", "14px")]:
            app.js(f"document.querySelector('input[name=classicFont][value=\"{key}\"]').click()")
            app.until(lambda: json.loads(cfg_file.read_text()).get("classicFont") == key, msg=key)
            app.until(lambda: app.js(f"return [...document.fonts].some(f => f.family.includes('{family}') && f.status === 'loaded')"), msg=f"{key} loaded")
            font_info[key] = (app.js("return getComputedStyle(document.body).fontFamily"), app.js("return getComputedStyle(document.body).fontSize"))
            if key == "dogica":
                app.shot("12h-settings-font-dogica")
        check("choosing a font switches the Classic look at once and is saved (Dogica 8 px, Geist Pixel and Roboto Slab 14 px)",
              all(font_info[k][0].startswith(f'"{fam}"') and font_info[k][1] == size for k, fam, size in
                  [("dogica", "Dogica Pixel", "8px"), ("geist-pixel", "Geist Pixel", "14px"), ("roboto-slab", "Roboto Slab", "14px")]), str(font_info))
        app.js("document.querySelector('input[name=classicFont][value=\"pixel-operator\"]').click()")
        app.until(lambda: json.loads(cfg_file.read_text()).get("classicFont") == "pixel-operator", msg="back to default")
        check("back to Pixel Operator", app.js("return getComputedStyle(document.body).fontSize") == "16px")
        # ---- the journal offer can be turned back on
        check("Settings: the journal offer after playing is off (turned off in the offer itself)",
              app.js("return document.getElementById('st-journal-prompt').checked") is False)
        app.click("#st-journal-prompt")
        app.until(lambda: json.loads(cfg_file.read_text()).get("journalPrompt") is True, msg="prompt on")
        check("…and can be turned on again", True)
        # ---- the built-in viewer can be switched off: PDFs go to the default application
        check("Settings: manuals and music open in Game Curator (on by default)",
              app.js("return document.getElementById('st-viewer').checked") is True)
        app.click("#st-viewer")
        app.until(lambda: json.loads(cfg_file.read_text()).get("builtinViewer") is False, msg="viewer off")
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 3)
        app.js("[...document.querySelectorAll('.card')].find(c => /System Shock/.test(c.innerText)).click()")
        app.until(lambda: app.count("#pf-content table.list tbody tr") >= 6, msg="file list")
        LOG.write_text("")
        app.js("[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('Manual.pdf')).querySelector('[data-open]').click()")
        app.until(lambda: any(a.startswith("OPEN") for a in actions()), msg="pdf external")
        time.sleep(0.5)
        check("…switched off: Open hands the PDF to the default application, no viewer window",
              any(a.endswith("Manual.pdf") for a in actions()) and len(app.d.window_handles) == 1 and "Opened with the default application" in app.text("#sb-msg"))
        app.click("#nav-settings")
        app.click("#st-viewer")
        app.until(lambda: json.loads(cfg_file.read_text()).get("builtinViewer") is True, msg="viewer on")
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 3)
        structure = "return [...document.querySelectorAll('button, input, select, a')].map(e => e.tagName + (e.id || e.className)).join('|')"
        classic_struct = app.js(structure)
        for theme in ["amber", "matrix", "dos"]:
            app.click("#nav-settings")
            app.css(f"input[name=theme][value={theme}]")
            app.js(f"document.querySelector('input[name=theme][value={theme}]').click()")
            app.until(lambda: json.loads((root_d / "Data" / "config.json").read_text()).get("theme") == theme, msg=f"theme {theme} saved")
            check(f"{theme}: theme applied and saved to config.json",
                  app.js("return document.documentElement.dataset.theme + ' ' + document.documentElement.classList.contains('term')") == f"{theme} true")
            app.shot(f"20-{theme}-settings")
            app.click("#nav-library")
            app.until(lambda: app.count(".card") == 3)
            check(f"{theme}: same controls as classic (appearance only)", app.js(structure) == classic_struct)
            body_font = app.js("return getComputedStyle(document.body).fontFamily")
            check(f"{theme}: monospace terminal font in use (Classic's pixel font does not leak)",
                  ("Consolas" in body_font or "monospace" in body_font) and "Pixel Operator" not in body_font, body_font)
            if theme == "dos":
                check("dos: IBM VGA text-mode font (PxPlus IBM VGA8) loaded, at its native 16px",
                      body_font.startswith('"PxPlus IBM VGA8"')
                      and app.js("return [...document.fonts].some(f => f.family.includes('PxPlus IBM VGA8') && f.status === 'loaded')")
                      and app.js("return getComputedStyle(document.querySelector('.card .t')).fontSize") == "16px",
                      body_font)
            app.shot(f"21-{theme}-library")
            app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('System Shock')).click()")
            app.until(lambda: app.count("#pf-content tbody tr") >= 6)
            app.shot(f"22-{theme}-profile")
            if theme in ("matrix", "dos"):
                main_win = app.d.current_window_handle
                app.js("[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('Main Theme')).querySelector('[data-open]').click()")
                app.new_window([main_win])
                app.until(lambda: app.count(".pl-track.on") == 1 and app.js("return document.getElementById('pl-audio').dataset.bytes || ''") != "", msg="player")
                time.sleep(0.4)
                app.shot(f"22b-{theme}-player")
                wm_close("Music$")
                app.until(lambda: len(app.d.window_handles) == 1, msg="player closed")
                app.d.switch_to.window(main_win)
                app.js("[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('Manual.pdf')).querySelector('[data-open]').click()")
                app.new_window([main_win])
                app.until(lambda: app.count(".vw-page.drawn") >= 1, msg="viewer")
                app.shot(f"22c-{theme}-viewer")
                wm_close("^Manual")
                app.until(lambda: len(app.d.window_handles) == 1, msg="viewer closed")
                app.d.switch_to.window(main_win)
            app.click("#pf-edit")
            app.visible("#gf-title")
            app.shot(f"23-{theme}-dialog")
            app.click(".overlay [data-close]")
            app.gone("#gf-title")
            app.click("#nav-journal")
            app.until(lambda: app.count(".entry") >= 1)
            app.shot(f"24-{theme}-journal")
            app.click("#nav-scanner")
            app.shot(f"25-{theme}-scanner")
        # leave the Matrix theme on: session 2 checks that it survives a restart
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 3)
        # closing the main window closes the music player too, and the app ends cleanly
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('System Shock')).click()")
        app.until(lambda: app.count("#pf-content tbody tr") >= 6)
        main_win = app.d.current_window_handle
        app.js("[...document.querySelectorAll('#pf-content tbody tr')].find(r=>r.innerText.includes('Main Theme')).querySelector('[data-open]').click()")
        app.new_window([main_win])
        app.until(lambda: app.count(".pl-track.on") == 1, msg="player before closing")
        app.closed = True
        wm_close("^Game Curator 0.1$")
        gone = False
        for _ in range(50):
            if subprocess.run(["pgrep", "-x", BIN.name[:15]], capture_output=True).returncode != 0:
                gone = True
                break
            time.sleep(0.2)
        check("closing the main window closes the player and the viewers with it; Game Curator ends", gone)
    except Exception:
        traceback.print_exc()
        check("session 1 completed without errors", False, traceback.format_exc(limit=2))
        app.shot("error-s1")
    finally:
        app.quit()

    # -------------------------------------------------- session 2: move archive (tests 1, 2, 5)
    section("Session 2 - archive moved / drive letter changed (tests 1, 2, 5)")
    drive_e = WORK / "E"
    shutil.move(str(drive_d), str(drive_e))
    root_e = drive_e / "GAME_CURATOR"
    # another computer: other name, no games installed there
    (WORK / "installed-none.json").write_text("[]")
    app = App(root_e, "s2", machine="OTHER-PC", installed=WORK / "installed-none.json")
    try:
        app.until(lambda: app.count(".card") == 3, msg="cards after move")
        check("on another computer no game counts as installed (installations are per PC)",
              app.count(".card .inst-box") == 3 and app.count(".card .inst-box.on") == 0)
        check("no wizard on second start", app.count(".wizard") == 0)
        check("no welcome window after “Don't show this again”", app.count(".welcome") == 0)
        check("chosen theme (DOS Midnight) restored at start", app.js("return document.documentElement.dataset.theme") == "dos")
        app.click("#nav-settings")
        app.css("input[name=theme][value=classic]")
        app.js("document.querySelector('input[name=theme][value=classic]').click()")
        app.until(lambda: app.js("return document.documentElement.dataset.theme") == "classic")
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 3)
        time.sleep(3)
        o, t_, _ = rss_kb()
        METRICS["idle_rss_3_games_app_mb"] = round(o / 1024, 1)
        METRICS["idle_rss_3_games_total_mb"] = round(t_ / 1024, 1)
        check("all games available after moving the archive", app.count(".card.offline") == 0)
        app.until(lambda: app.js("return [...document.querySelectorAll('.card img')].every(i=>i.complete&&i.naturalWidth>0) && document.querySelectorAll('.card img').length==2"), msg="artworks")
        check("artworks load from the moved archive", True)
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('System Shock')).click()")
        app.until(lambda: "Medical deck (edited)" in app.text("#pf-journal"))
        check("journal entry survived restart and move", True)
        app.until(lambda: app.js("const i=[...document.querySelectorAll('#pf-shots .shot img')]; return i.length===3 && i.every(x=>x.complete&&x.naturalWidth>0)"), msg="shots after move")
        check("screenshots survived the move (served from the new drive)", "All 5 screenshots" in app.text("#pf-shots"))
        check("'What I know about this game' survived", "GOG offline installer v2.48" in app.css("#pf-notes").get_attribute("value"))
        app.until(lambda: app.count("#pf-content tbody tr") >= 6)
        check("external folder (../GOG_Installers) resolves on the new drive", True)
        path_txt = app.text("#pf-content .pathline .mono")
        check("paths now point to drive E", "/E/" in path_txt, path_txt)
        app.shot("13-after-move")
    except Exception:
        traceback.print_exc()
        check("session 2 completed without errors", False, traceback.format_exc(limit=2))
        app.shot("error-s2")
    finally:
        app.quit()

    # -------------------------------------------------- session 3: unavailable drive (test 7)
    section("Session 3 - unavailable game drive and disconnected archive (test 7)")
    shutil.move(str(drive_e / "GOG_Installers"), str(WORK / "GOG_unplugged"))
    app = App(root_e, "s3")
    try:
        app.until(lambda: app.count(".card") == 3)
        app.until(lambda: app.count(".card.offline") == 2, timeout=20, msg="offline cards")
        check("games on the missing drive are marked OFFLINE, not deleted", app.count(".card") == 3)
        check("back on the first PC: detected and located installations are known again", app.count(".card .inst-box.on") == 2)
        app.until(lambda: "2 offline" in app.text("#sb-count"), timeout=10, msg="offline count")
        check("status bar counts the offline games", "2 offline" in app.text("#sb-count"), app.text("#sb-count"))
        app.shot("14-offline-games")
        app.js("[...document.querySelectorAll('.card')].find(c=>c.innerText.includes('Disco')).click()")
        app.visible("#pf-content .notice.warn")
        check("profile explains that the folder is not available", "not available" in app.text("#pf-content"))
        db = sqlite3.connect(root_e / "Data" / "library.db")
        check("file records of the unavailable game are kept", db.execute("select count(*) from files f join games g on g.id=f.game_id where g.title='Disco Elysium'").fetchone()[0] >= 6)
        db.close()
        # archive drive disconnected while running
        shutil.move(str(root_e / "Data"), str(root_e / "Data_unplugged"))
        app.js("window.dispatchEvent(new Event('focus'))")
        app.until(lambda: "NOT AVAILABLE" in app.text("#sb-archive"), timeout=10)
        check("status bar reports the disconnected archive", True)
        app.click("#nav-library")
        time.sleep(0.5)
        app.shot("15-archive-disconnected")
        app.close_modals()
        shutil.move(str(root_e / "Data_unplugged"), str(root_e / "Data"))
        app.js("window.dispatchEvent(new Event('focus'))")
        app.until(lambda: "NOT AVAILABLE" not in app.text("#sb-archive"), timeout=10)
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 3)
        check("after reconnecting, the archive works again with all records", True)
    except Exception:
        traceback.print_exc()
        check("session 3 completed without errors", False, traceback.format_exc(limit=2))
        app.shot("error-s3")
    finally:
        app.quit()
    shutil.move(str(WORK / "GOG_unplugged"), str(drive_e / "GOG_Installers"))
    db = sqlite3.connect(root_e / "Data" / "library.db")
    check("database intact after all sessions (4 games incl. removed one gone -> 3)", db.execute("select count(*) from games").fetchone()[0] == 3)
    check("integrity check ok", db.execute("pragma integrity_check").fetchone()[0] == "ok")
    db.close()

    # -------------------------------------------------- session 4: 500 games (test 8)
    section("Session 4 - performance with 500 games (test 8)")
    perf_root = WORK / "P" / "GAME_CURATOR"
    lib = WORK / "P" / "Library500"
    t = time.perf_counter()
    for i in range(500):
        touch(lib / f"Game {i:03d}" / f"setup_game_{i:03d}.exe", 128)
        touch(lib / f"Game {i:03d}" / "Manual.pdf", 64)
    METRICS["fixture_folders_s"] = round(time.perf_counter() - t, 2)
    app = App(perf_root, "s4a")
    try:
        app.dismiss_welcome()
        app.visible(".wizard")
        app.click("#wz-skip")
        app.click("#nav-scanner")
        app.type("#sc-root", str(lib))
        t = time.perf_counter()
        app.click("#sc-start")
        app.until(lambda: app.count("#sc-results tbody tr") == 500, timeout=120, msg="500 candidates")
        METRICS["scan_500_folders_s"] = round(time.perf_counter() - t, 2)
        app.until(lambda: app.css("#sc-import").is_enabled())
        t = time.perf_counter()
        app.click("#sc-import")
        app.visible(".overlay [data-ok]", timeout=300)
        METRICS["import_500_games_s"] = round(time.perf_counter() - t, 2)
        check("500 games imported via the scanner", "500 game(s) imported" in app.text(".overlay .dlg-text"), app.text(".overlay .dlg-text"))
        app.click(".overlay [data-ok]")
    except Exception:
        traceback.print_exc()
        check("perf import completed", False, traceback.format_exc(limit=2))
        app.shot("error-s4a")
    finally:
        app.quit()

    # give every game a cover (direct DB update, simulating artwork uploads)
    t = time.perf_counter()
    db = sqlite3.connect(perf_root / "Data" / "library.db")
    ids = [r[0] for r in db.execute("select id from games order by id")]
    for n, gid in enumerate(ids):
        name = f"cover-perf{gid:011d}.webp"
        col = ((n * 37) % 200 + 30, (n * 61) % 200 + 30, (n * 97) % 200 + 30)
        img = Image.new("RGB", (320, 427), col)  # same size the app produces for a 3:4 cover
        img.save(perf_root / "Data" / "Thumbnails" / name, "WEBP", quality=80)
        img.save(perf_root / "Data" / "Artwork" / "Covers" / name, "WEBP", quality=88)
        db.execute("update games set artwork_path=?, thumbnail_path=?, genre=?, release_year=? where id=?",
                   (f"Data/Artwork/Covers/{name}", f"Data/Thumbnails/{name}", ["RPG", "Strategy", "Action", "Adventure"][n % 4], 1990 + n % 35, gid))
    db.commit()
    db.close()
    METRICS["fixture_covers_s"] = round(time.perf_counter() - t, 2)

    app = App(perf_root, "s4b")
    try:
        t0 = time.perf_counter()
        app.until(lambda: app.count(".card") == 500, timeout=60, msg="500 cards")
        METRICS["startup_to_500_cards_s"] = round(app.launch_s + (time.perf_counter() - t0), 2)
        METRICS["webdriver_session_launch_s"] = round(app.launch_s, 2)
        METRICS["dom_ready_to_cards_ms"] = app.js("return Math.round(performance.now())")
        time.sleep(3)
        own0, total0, _ = rss_kb()
        METRICS["idle_rss_after_start_500_games_app_mb"] = round(own0 / 1024, 1)
        METRICS["idle_rss_after_start_500_games_total_mb"] = round(total0 / 1024, 1)
        loaded_initial = app.js("return [...document.querySelectorAll('.card img')].filter(i=>i.complete&&i.naturalWidth>0).length")
        METRICS["images_loaded_before_scrolling"] = loaded_initial
        check("lazy loading: only visible artworks are loaded at start", loaded_initial < 200, str(loaded_initial))
        # scrolling smoothness: frames during programmatic scroll
        scroll_js = """
            const done = arguments[arguments.length-1];
            const el = document.getElementById('lib-scroll');
            let frames = 0, worst = 0, last = performance.now();
            const start = last, max = el.scrollHeight - el.clientHeight;
            function step(now){
              frames++; worst = Math.max(worst, now - last); last = now;
              const p = (now - start) / 3000;
              el.scrollTop = Math.min(1, p) * max;
              if (p < 1) requestAnimationFrame(step); else done({fps: Math.round(frames / ((now - start)/1000)), worstFrameMs: Math.round(worst)});
            }
            el.scrollTop = 0;
            requestAnimationFrame(step);"""
        fps = app.d.execute_async_script(scroll_js)
        METRICS["grid_scroll_fps_first_pass"] = fps["fps"]
        METRICS["grid_scroll_worst_frame_ms_first_pass"] = fps["worstFrameMs"]
        fps2 = app.d.execute_async_script(scroll_js)
        METRICS["grid_scroll_fps_second_pass"] = fps2["fps"]
        METRICS["grid_scroll_worst_frame_ms_second_pass"] = fps2["worstFrameMs"]
        for theme in ["amber", "matrix", "dos"]:
            app.click("#nav-settings")
            app.css(f"input[name=theme][value={theme}]")
            app.js(f"document.querySelector('input[name=theme][value={theme}]').click()")
            app.until(lambda: app.js("return document.documentElement.dataset.theme") == theme)
            app.click("#nav-library")
            app.until(lambda: app.count(".card") == 500)
            r = app.d.execute_async_script(scroll_js)
            METRICS[f"grid_scroll_fps_{theme}"] = r["fps"]
            METRICS[f"grid_scroll_worst_frame_ms_{theme}"] = r["worstFrameMs"]
            app.shot(f"26-{theme}-library-500")
        app.click("#nav-settings")
        app.css("input[name=theme][value=classic]")
        app.js("document.querySelector('input[name=theme][value=classic]').click()")
        app.click("#nav-library")
        app.until(lambda: app.count(".card") == 500)
        time.sleep(1.5)
        METRICS["images_loaded_after_scrolling"] = app.js("return [...document.querySelectorAll('.card img')].filter(i=>i.complete&&i.naturalWidth>0).length")
        # search latency: input -> DOM updated
        lat = app.d.execute_async_script("""
            const done = arguments[arguments.length-1];
            const input = document.getElementById('lib-search');
            const grid = document.getElementById('lib-scroll');
            const t0 = performance.now();
            const obs = new MutationObserver(() => {
              if (document.querySelectorAll('.card').length === 1) { obs.disconnect(); done(Math.round(performance.now() - t0)); }
            });
            obs.observe(grid, {childList: true, subtree: true});
            input.value = 'game 042'; input.dispatchEvent(new Event('input', {bubbles: true}));""")
        METRICS["search_latency_ms_incl_140ms_debounce"] = lat
        check("search over 500 games answers fast", lat < 600, f"{lat} ms")
        app.css("#lib-search").clear()
        app.js("const i=document.getElementById('lib-search'); i.value=''; i.dispatchEvent(new Event('input',{bubbles:true}))")
        app.until(lambda: app.count(".card") == 500)
        t = time.perf_counter()
        app.click("[data-mode='catalog']")
        app.until(lambda: app.count("table.list tbody tr") == 500)
        METRICS["switch_to_catalog_500_ms"] = round((time.perf_counter() - t) * 1000)
        app.click("[data-mode='grid']")
        app.until(lambda: app.count(".card") == 500)
        app.shot("16-library-500")
        time.sleep(3)
        own, total, nproc = rss_kb()
        METRICS["idle_rss_after_scrolling_all_500_app_mb"] = round(own / 1024, 1)
        METRICS["idle_rss_after_scrolling_all_500_total_mb"] = round(total / 1024, 1)
        METRICS["process_count"] = nproc
    except Exception:
        traceback.print_exc()
        check("perf session completed", False, traceback.format_exc(limit=2))
        app.shot("error-s4b")
    finally:
        app.quit()
    time.sleep(1)
    left = subprocess.run(["pgrep", "-f", str(BIN)], capture_output=True, text=True).stdout.strip()
    check("no process keeps running after the app is closed", left == "", left)

    bin_size = BIN.stat().st_size
    METRICS["binary_size_mb_linux"] = round(bin_size / 1024 / 1024, 1)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    try:
        run()
    except Exception:
        traceback.print_exc()
        check("test run completed", False, traceback.format_exc(limit=3))
    finally:
        passed = sum(1 for r in RESULTS if r.get("ok") is True)
        failed = [r for r in RESULTS if r.get("ok") is False]
        (OUT / "results.json").write_text(json.dumps({"results": RESULTS, "metrics": METRICS}, indent=2))
        print("\nMETRICS:", json.dumps(METRICS, indent=2))
        print(f"\nRESULT: {passed} passed, {len(failed)} failed")
        sys.exit(1 if failed else 0)
