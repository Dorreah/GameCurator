#!/usr/bin/env python3
"""Checks the Czech translation of the interface.

* Every English key used in the UI - t('...'), tn(n, '...', '...'),
  mark('...') and data-i18n* attributes in the HTML pages - must have a
  Czech entry in ui/js/lang/cs.js (CS).
* Every user-facing message written in Rust (errors, notes) must have an
  entry in CS_MESSAGES, matched exactly or by pattern ({0}, {1} = the
  variable parts).
* Placeholders ({n}, {title}, ...) must be the same in key and translation,
  and plural entries ([one, few, many]) must have three forms.

Unused entries are reported as warnings. Exit code 1 when something is
missing. Needs Node.js (to read the dictionary module).

    python3 tools/check_i18n.py
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "ui"

# ---------------------------------------------------------------- JS string literals

ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "b": "\b", "f": "\f", "v": "\v"}


def unescape(body):
    def repl(m):
        e = m.group(1)
        if e.startswith("u{"):
            return chr(int(e[2:-1], 16))
        if e.startswith("u"):
            return chr(int(e[1:], 16))
        if e.startswith("x"):
            return chr(int(e[1:], 16))
        if e == "\n":
            return ""
        return ESCAPES.get(e, e)

    return re.sub(r"\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|.|\n)", repl, body, flags=re.S)


def read_literal(src, i):
    """String literal starting at src[i] -> (value or None if dynamic, end index)."""
    q = src[i]
    j = i + 1
    while j < len(src):
        c = src[j]
        if c == "\\":
            j += 2
            continue
        if q == "`" and c == "$" and src[j + 1 : j + 2] == "{":
            return None, j
        if c == q:
            return unescape(src[i + 1 : j]), j + 1
        j += 1
    return None, j


def skip_ws(src, i):
    while i < len(src) and src[i] in " \t\r\n":
        i += 1
    return i


def skip_expression(src, i):
    """Skips one call argument (up to the top-level comma or closing paren)."""
    depth = 0
    while i < len(src):
        c = src[i]
        if c in "'\"`":
            _, i = read_literal(src, i)
            if src[i - 1 : i] not in ("'", '"', "`"):  # template with ${} - skip to its end
                while i < len(src) and src[i] != "`":
                    i += 1
                i += 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                return i
            depth -= 1
        elif c == "," and depth == 0:
            return i
        i += 1
    return i


def keys_in_js(src):
    keys = set()
    for m in re.finditer(r"(?<![\w.$])(t|tn|mark)\(", src):
        fn, i = m.group(1), m.end()
        i = skip_ws(src, i)
        if fn == "tn":
            i = skip_expression(src, i)  # the number
            if src[i : i + 1] != ",":
                continue
            i = skip_ws(src, i + 1)
            if i >= len(src) or src[i] not in "'\"`":
                continue
            _one, i = read_literal(src, i)  # singular (English only)
            i = skip_ws(src, i)
            if src[i : i + 1] != ",":
                continue
            i = skip_ws(src, i + 1)
        if i < len(src) and src[i] in "'\"`":
            value, _ = read_literal(src, i)
            if value is not None:
                keys.add(value)
    return keys


def keys_in_html(src):
    keys = set()
    for m in re.finditer(r'data-i18n(?:-title|-placeholder|-aria|-doc)?="([^"]*)"', src):
        keys.add(m.group(1).replace("&amp;", "&").replace("&quot;", '"'))
    return keys


# ---------------------------------------------------------------- Rust messages

RUST_PATTERNS = [
    r'CoreError::(?:invalid|not_found)\(\s*(?:format!\()?\s*"((?:[^"\\]|\\.)*)"',
    r'Err\(\s*(?:format!\()?\s*"((?:[^"\\]|\\.)*)"\s*\)?\.to_string\(\)',
    r'Err\(\s*format!\(\s*"((?:[^"\\]|\\.)*)"',
    r'\.ok_or\(\s*"((?:[^"\\]|\\.)*)"',
    r'ok_or_else\(\|\|\s*"((?:[^"\\]|\\.)*)"\.to_string\(\)',
    r'#\[error\("((?:[^"\\]|\\.)*)"\)\]',
    r'write!\(f,\s*"((?:[^"\\]|\\.)*)"',
    r'notes\.push\(format!\(\s*"((?:[^"\\]|\\.)*)"',
    r'=>\s*"((?:[^"\\]|\\.)*)"\.to_string\(\)',
    r'=>\s*format!\(\s*"((?:[^"\\]|\\.)*)"',
    r'map_err\(\|e\|\s*format!\(\s*"((?:[^"\\]|\\.)*)"',
]
RUST_SKIP = {"{0}", "{e}", "", "catalog: no products", "redirect without address", "answer too large",
             "too many redirects", "not text", "product id", "bad path", "not found"}


def rust_unescape(s):
    s = re.sub(r"\\u\{([0-9a-fA-F]+)\}", lambda m: chr(int(m.group(1), 16)), s)
    s = s.replace("\\n", "\n").replace('\\"', '"').replace("\\\\", "\\")
    return re.sub(r"\\\n\s*", "", s)


def rust_messages():
    found = {}
    files = list((ROOT / "crates" / "curator-core" / "src").glob("*.rs")) + list((ROOT / "src-tauri" / "src").glob("*.rs"))
    for f in files:
        src = f.read_text(encoding="utf-8")
        src = src.split("#[cfg(test)]")[0]  # unit tests are not user-facing
        for pat in RUST_PATTERNS:
            for m in re.finditer(pat, src):
                msg = rust_unescape(m.group(1))
                n = iter(range(10))
                msg = re.sub(r"\{[^{}]*\}", lambda _m: "{%d}" % next(n), msg)
                if msg in RUST_SKIP or msg.startswith("{0}") and len(msg) < 6:
                    continue
                if not re.search(r"[A-Za-z]{3}", msg) or re.search(r"\b(SELECT|WHERE|ORDER BY)\b", msg):
                    continue
                found.setdefault(msg, f.name)
    return found


def matches(msg, keys):
    if msg in keys:
        return True
    for k in keys:
        if "{0}" not in k:
            continue
        parts = [re.escape(p) for p in re.split(r"\{\d\}", k)]
        if re.fullmatch("(.*?)".join(parts), msg, flags=re.S):
            return True
    return False


# ---------------------------------------------------------------- main

def placeholders(s):
    return sorted(set(re.findall(r"\{(\w+)\}", s)))


def main():
    out = subprocess.run(
        ["node", "-e", "import(process.argv[1]).then(m => process.stdout.write(JSON.stringify({cs: m.CS, msg: m.CS_MESSAGES})))",
         (UI / "js" / "lang" / "cs.js").as_uri()],
        capture_output=True, text=True, check=True,
    )
    data = json.loads(out.stdout)
    cs, msgs = data["cs"], data["msg"]

    used = set()
    for f in sorted((UI / "js").rglob("*.js")):
        if "lang" in f.parts:
            continue
        used |= keys_in_js(f.read_text(encoding="utf-8"))
    for f in sorted(UI.glob("*.html")):
        used |= keys_in_html(f.read_text(encoding="utf-8"))

    problems = 0
    # a key written twice in one dictionary: the later entry silently wins
    src = (UI / "js" / "lang" / "cs.js").read_text(encoding="utf-8")
    split = src.find("CS_MESSAGES")
    for part in (src[:split], src[split:]):
        seen = set()
        for m in re.finditer(r"^\s{2}('(?:[^'\\]|\\.)*'|\"(?:[^\"\\]|\\.)*\")\s*:", part, re.M):
            k = m.group(1)
            if k in seen:
                problems += 1
                print(f"DUPLICATE {k} (the later entry wins)")
            seen.add(k)
    missing = sorted(k for k in used if k not in cs)
    for k in missing:
        print(f"MISSING  {k!r}")
    problems += len(missing)

    for k, v in cs.items():
        forms = v if isinstance(v, list) else [v]
        if isinstance(v, list) and len(v) != 3:
            print(f"PLURAL   {k!r} needs [one, few, many]")
            problems += 1
        for form in forms:
            if placeholders(form) and placeholders(form) != placeholders(k) and not set(placeholders(form)) <= set(placeholders(k)) | {"n"}:
                print(f"PLACEHOLDER {k!r} -> {form!r}")
                problems += 1
            elif set(placeholders(k)) - {"n"} - set(placeholders(form)):
                print(f"PLACEHOLDER lost in {k!r} -> {form!r}")
                problems += 1
    for k, v in msgs.items():
        if sorted(set(re.findall(r"\{\d\}", k))) != sorted(set(re.findall(r"\{\d\}", v))):
            print(f"PLACEHOLDER message {k!r} -> {v!r}")
            problems += 1

    rust = rust_messages()
    for msg, where in sorted(rust.items()):
        if not matches(msg, msgs):
            print(f"RUST     {msg!r}  ({where})")
            problems += 1

    unused = sorted(k for k in cs if k not in used)
    for k in unused:
        print(f"unused   {k!r}")

    print(f"\n{len(used)} interface texts, {len(cs)} Czech entries, {len(rust)} Rust messages, {len(msgs)} message entries.")
    if problems:
        print(f"{problems} problem(s).")
        return 1
    print("Czech translation complete.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
