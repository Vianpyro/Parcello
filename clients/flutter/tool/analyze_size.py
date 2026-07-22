#!/usr/bin/env python3
"""Bundle size analysis for the Flutter WEB build.

Web is not supported by `flutter build web --analyze-size` (that flag is
AOT-only). This is the web-appropriate equivalent: per-asset raw/gzip/Brotli
sizes, a category rollup, the first-load total, and a per-Dart-library
attribution of main.dart.js parsed from its source map (build with
`--source-maps`). Writes both a human report (.txt) and a machine one (.json).

Usage: tool/analyze_size.py [BUILD_DIR] [OUT_DIR]
Deps: gzip (stdlib). Brotli via the `brotli` python module if present, else the
`brotli` CLI; if neither is available the Brotli column is omitted.
"""
import gzip as _gz
import json
import os
import subprocess
import sys
from collections import defaultdict

BUILD = sys.argv[1] if len(sys.argv) > 1 else "build/web"
OUT = sys.argv[2] if len(sys.argv) > 2 else "perf-report"
os.makedirs(OUT, exist_ok=True)

# --- Brotli backend: module -> CLI -> unavailable ---------------------------
try:
    import brotli as _brmod

    def brotli_len(data):
        return len(_brmod.compress(data, quality=11))
    BR = "module"
except Exception:
    from shutil import which
    if which("brotli"):
        def brotli_len(data):
            p = subprocess.run(["brotli", "-q", "11", "-c"], input=data,
                               stdout=subprocess.PIPE, check=True)
            return len(p.stdout)
        BR = "cli"
    else:
        def brotli_len(data):
            return None
        BR = None


PRIORITY = {
    "main.dart.js", "flutter.js", "flutter_bootstrap.js",
    "canvaskit/chromium/canvaskit.js", "canvaskit/chromium/canvaskit.wasm",
}


def want_compress(rel, raw):
    # Brotli(q11) is slow; only compute where it's meaningful -- the files the
    # browser actually fetches, plus anything small. The 5 unfetched renderer
    # variants (multi-MB wasm) are skipped; their raw size is still reported.
    return rel in PRIORITY or rel.endswith((".ttf", ".otf")) or raw < 1_500_000


def sizes(path, rel):
    with open(path, "rb") as f:
        data = f.read()
    raw = len(data)
    if want_compress(rel, raw):
        return raw, len(_gz.compress(data, 9)), brotli_len(data)
    return raw, None, None


def human(n):
    if n is None:
        return "-"
    n = float(n)
    for u in ("B", "KB", "MB"):
        if n < 1024 or u == "MB":
            return f"{int(n)} B" if u == "B" else f"{n:.1f} {u}"
        n /= 1024


def category(rel):
    p = rel.lower()
    if p.startswith("canvaskit/"):
        return "Renderer variants (one fetched)"
    if p.endswith((".ttf", ".otf")):
        return "Fonts"
    if p.endswith(".mp3") or "/sfx/" in p:
        return "Audio (sfx)"
    if p == "main.dart.js":
        return "App code (main.dart.js)"
    if p.endswith(".js"):
        return "Loader JS"
    if p.endswith(".wasm"):
        return "WASM (other)"
    if p.endswith((".png", ".ico")):
        return "Icons"
    return "Shell / manifests / other"


rows = []
for dp, _, files in os.walk(BUILD):
    for fn in files:
        full = os.path.join(dp, fn)
        rel = os.path.relpath(full, BUILD)
        try:
            raw, g, b = sizes(full, rel)
        except OSError:
            continue
        rows.append({"file": rel, "raw": raw, "gzip": g, "brotli": b,
                     "category": category(rel)})
rows.sort(key=lambda r: r["raw"], reverse=True)

# --- category rollup --------------------------------------------------------
cats = defaultdict(lambda: [0, 0, 0, 0])
for r in rows:
    c = cats[r["category"]]
    c[0] += r["raw"]
    c[1] += (r["gzip"] or 0)
    c[2] += (r["brotli"] or 0)
    c[3] += 1

# --- first-load model (Chromium: one renderer, all fonts eager) -------------
FIRST = [
    "index.html", "flutter.js", "flutter_bootstrap.js", "main.dart.js",
    "canvaskit/chromium/canvaskit.js", "canvaskit/chromium/canvaskit.wasm",
    "assets/fonts/MaterialIcons-Regular.otf",
    # The loading-splash logo (index.html) -- real first-load fetch, see
    # docs/web-performance.md, "Why the loading splash exists".
    "icons/Icon-192.png",
]
FIRST += [r["file"] for r in rows
          if r["file"].startswith("assets/assets/fonts/") and r["file"].endswith(".ttf")]
FIRST += ["assets/AssetManifest.bin", "assets/AssetManifest.bin.json",
          "assets/FontManifest.json", "version.json", "manifest.json"]
by_file = {r["file"]: r for r in rows}
fl = [by_file[f] for f in FIRST if f in by_file]
fl_raw = sum(r["raw"] for r in fl)
fl_gz = sum(r["gzip"] for r in fl)
fl_br = sum((r["brotli"] or 0) for r in fl)

# --- source-map attribution of main.dart.js ---------------------------------
def attribute_mapfile():
    mp = os.path.join(BUILD, "main.dart.js.map")
    js = os.path.join(BUILD, "main.dart.js")
    if not (os.path.exists(mp) and os.path.exists(js)):
        return None
    B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
    idx = {c: i for i, c in enumerate(B64)}

    def vlq(seg):
        out, shift, res = [], 0, 0
        for c in seg:
            d = idx[c]
            cont = d & 32
            d &= 31
            res += d << shift
            if cont:
                shift += 5
            else:
                v = res >> 1
                out.append(-v if res & 1 else v)
                shift = res = 0
        return out

    m = json.load(open(mp))
    srcs = m["sources"]
    line_len = [len(l) + 1 for l in open(js, "rb").read().split(b"\n")]

    def bucket(s):
        if "flutter/packages/flutter_localizations/" in s:
            return "flutter_localizations (all locales)"
        if "flutter/packages/flutter/" in s:
            return "Flutter framework + Material"
        if "org-dartlang-sdk:///lib/_engine" in s or "org-dartlang-sdk:///lib/web_ui" in s:
            return "Flutter Web engine (incl. font-fallback tables)"
        if "org-dartlang-sdk:///lib/ui/" in s:
            return "dart:ui"
        if "org-dartlang-sdk" in s or "/dart-sdk/" in s:
            return "Dart SDK / runtime"
        if "/audioplayers" in s:
            return "audioplayers"
        if "/intl-" in s or "/intl/" in s:
            return "intl"
        if "/material_color_utilities" in s:
            return "material_color_utilities"
        if ".pub-cache" in s or "/pub.dev/" in s or "/hosted/" in s or "/packages/" in s:
            return "other pub packages"
        if s.endswith(".dart") and "org-dartlang" not in s:
            return "Parcello app code (lib/)"
        return "other / runtime glue"

    per = defaultdict(int)
    si = 0
    for gi, group in enumerate(m["mappings"].split(";")):
        if not group:
            continue
        dec, gc = [], 0
        for seg in group.split(","):
            if not seg:
                continue
            v = vlq(seg)
            gc += v[0]
            if len(v) >= 4:
                si += v[1]
                dec.append((gc, si))
            else:
                dec.append((gc, None))
        ll = line_len[gi] if gi < len(line_len) else 0
        for j, (c, s) in enumerate(dec):
            end = dec[j + 1][0] if j + 1 < len(dec) else ll
            run = end - c
            if run > 0 and s is not None and 0 <= s < len(srcs):
                per[bucket(srcs[s])] += run
    return dict(sorted(per.items(), key=lambda kv: kv[1], reverse=True))


attribution = attribute_mapfile()

# --- write JSON -------------------------------------------------------------
report = {
    "build_dir": BUILD,
    "brotli_backend": BR,
    "files": rows,
    "categories": {k: {"raw": v[0], "gzip": v[1], "brotli": v[2], "count": v[3]}
                   for k, v in cats.items()},
    "first_load": {"raw": fl_raw, "gzip": fl_gz, "brotli": fl_br,
                   "files": [r["file"] for r in fl]},
    "main_dart_js_attribution": attribution,
}
with open(os.path.join(OUT, "size-analysis.json"), "w") as f:
    json.dump(report, f, indent=2)

# --- write / print human report --------------------------------------------
lines = []
def p(s=""):
    lines.append(s)

if BR is None:
    p("NOTE: no Brotli backend found; Brotli column omitted (pip install brotli, or apt install brotli).")
p("FLUTTER WEB BUNDLE SIZE ANALYSIS")
p("(web equivalent of --analyze-size, which is AOT-only; see docs/web-performance.md)")
p("=" * 84)
p(f"{'FILE':<46}{'RAW':>11}{'GZIP':>11}{'BROTLI':>13}")
p("-" * 84)
for r in rows:
    if r["raw"] < 2048:
        continue
    p(f"{r['file'][:45]:<46}{human(r['raw']):>11}{human(r['gzip']):>11}{human(r['brotli']):>13}")
p("\nCATEGORY ROLLUP")
p("-" * 84)
for c, (raw, g, b, n) in sorted(cats.items(), key=lambda kv: kv[1][0], reverse=True):
    p(f"{c:<46}{human(raw):>11}{human(g):>11}{human(b):>13}")
p("\nFIRST LOAD (Chromium / Steam Deck: one renderer, fonts eager)")
p("-" * 84)
p(f"{'first-paint total':<46}{human(fl_raw):>11}{human(fl_gz):>11}{human(fl_br):>13}")
if attribution:
    total_attr = sum(attribution.values())
    js_raw = by_file.get("main.dart.js", {}).get("raw", 0)
    mapped_pct = f"{100 * total_attr / js_raw:.0f}" if js_raw else "?"
    p(f"\nmain.dart.js ATTRIBUTION (source map; {mapped_pct}% of bytes mapped)")
    p("-" * 84)
    for b, n in attribution.items():
        p(f"{b:<46}{human(n):>11}{100 * n / total_attr:>10.1f}%")

text = "\n".join(lines)
with open(os.path.join(OUT, "size-analysis.txt"), "w") as f:
    f.write(text + "\n")
print(text)
