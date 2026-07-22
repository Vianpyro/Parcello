# Web performance pipeline

How the Flutter Web client's load weight and Lighthouse scores are measured,
gated, and kept from regressing. Companion to `docs/performance.md` (which is
about the Rust engine/server); this file is only about the browser bundle.

Baseline was measured 2026-07-22 with **Flutter 3.44.6** (`flutter build web
--release`, the Dockerfile-pinned version). See the audit that produced these
numbers before changing budgets.

## What runs

Workflow: [`.github/workflows/web-perf.yml`](../.github/workflows/web-perf.yml),
triggered on `clients/flutter/**` changes (PR + push to `main`) and
`workflow_dispatch`. It is a **separate** workflow from `flutter.yml` on purpose:
the Node/Chrome/Lighthouse toolchain is heavy and must not slow the lean
`analyze + test` gate or the Rust CI.

One job, in order:

1. **Build** — `flutter build web --release --source-maps` (source maps let the
   size analyzer attribute `main.dart.js` to Dart libraries). Flutter is pinned
   to 3.44.6 so sizes and timings are reproducible.
2. **Size analysis** — `tool/analyze_size.py` (see the `--analyze-size` note
   below). Writes `perf-report/size-analysis.{txt,json}`.
3. **Gate 1 — Brotli size budgets** — `tool/size_budget.sh` checks
   `perf-budgets.json`. Fails the run if any budget is exceeded.
4. **Precompress** — writes `.br`/`.gz` siblings so the served build carries
   `Content-Encoding`, giving Lighthouse production-like transfer sizes.
5. **Gate 2 — Lighthouse CI** — `lhci autorun` serves the build
   (`http-server --brotli --gzip`) and asserts the five score/timing budgets in
   `lighthouserc.json`. Fails the run on any breach.
6. **Artifacts** — the whole `perf-report/` directory is uploaded (always, even
   on failure) as `web-perf-report`.

## About `flutter build web --analyze-size`

**`--analyze-size` does not exist for the web target** — it is an
AOT-only flag (apk/appbundle/ios/macos/windows/linux). `flutter build web
--analyze-size` fails with *"Could not find an option named --analyze-size"*.

The web-appropriate equivalent, which this pipeline uses, is
`flutter build web --release --source-maps` followed by
[`tool/analyze_size.py`](../clients/flutter/tool/analyze_size.py). It reports the
same information you would want from `--analyze-size`:

- per-asset **raw / gzip / Brotli** sizes and a category rollup;
- the **first-load total** a new Chromium/Steam-Deck visitor downloads (one
  renderer variant, fonts eager);
- a **per-Dart-library attribution** of `main.dart.js`, parsed from its source
  map (the same technique as `source-map-explorer` / `dart2js_info`).

## Reading the reports

Download the `web-perf-report` artifact from the workflow run.

### `size-analysis.txt` / `.json`
The bundle breakdown. Three things to look at:

- **First-load total** — the headline. Baseline: **11.1 MB raw / 4.2 MB gzip /
  3.2 MB Brotli**. This is what a first-time visitor downloads. The server must
  send it Brotli-compressed (see the audit's lever #1) for the 3.2 MB figure to
  be what actually crosses the wire.
- **Category rollup** — where the weight sits. `Renderer variants` is large on
  disk (~37 MB) but the browser fetches **one** (~5.6 MB); the rest are never
  requested (deploy-size only).
- **`main.dart.js` attribution** — why the app JS is ~705 KB Brotli. It is
  almost entirely **Flutter itself** (framework 43%, web engine 21%, Dart
  runtime 13%, `flutter_localizations` 10%). Parcello's own code is only ~5%
  (~100 KB) — there is no heavy app library to trim; the app-side lever is
  **font subsetting**, not code.

### `size-budget.txt`
Gate 1's table: each budget's measured Brotli size vs its limit, `PASS`/`FAIL`.
Deterministic and server-independent — it Brotli-compresses the build output
itself, so it measures the size a correct server *would* transfer.

### `lighthouse/`
Gate 2's Lighthouse reports — one HTML + JSON per run (3 runs, median asserted).
Open the HTML for the full waterfall, Performance/Accessibility/Best-Practices
breakdown, and the FCP/LCP timings. The `assertion-results.json` lists exactly
which budget failed and by how much.

## The budgets and how to adjust them

There are two budget files. **Adjusting a budget is a normal, expected action** —
tighten as optimizations land, loosen only with a reason noted in the diff.

### Size budgets — `clients/flutter/perf-budgets.json`
Each entry is a Brotli-transfer ceiling over a set of globs (relative to
`build/web`):

| Budget | Limit | Measured | Rationale |
|---|--:|--:|---|
| Application JS (`main.dart.js`) | 800 KB | 705 KB | The "JS transferred (Brotli)" budget. |
| Scripts fetched on first load | 780 KB | 735 KB | Only the JS Chromium actually loads. |
| Custom fonts | 1000 KB | 978 KB | Lower to ~450 once fonts are subset. |
| Renderer (CanvasKit chromium wasm) | 1650 KB | 1597 KB | ~1150 KB if you adopt Skwasm (`--wasm`). |
| First-load total | 3400 KB | 3200 KB | Overall guard. |

**Ratchet policy** (mirrors the coverage gate in `CLAUDE.md`): keep a small
headroom above the measured value so noise does not flake the gate, but when an
optimization lands, **lower the budget** to lock in the win. Example: after
subsetting fonts, drop `Custom fonts` from 1000 to ~450 and `First-load total`
accordingly.

To change a limit, edit `maxBrotliKB`. To budget a new asset group, add an entry
with a `label`, `globs`, and `maxBrotliKB`. Run it locally (below) to see the
new number before committing.

### Lighthouse budgets — `clients/flutter/lighthouserc.json`
The five hard assertions requested:

| Assertion | Budget |
|---|---|
| `categories:performance` | ≥ 0.90 |
| `categories:accessibility` | ≥ 0.95 |
| `categories:best-practices` | ≥ 0.95 |
| `first-contentful-paint` | ≤ 3000 ms |
| `largest-contentful-paint` | ≤ 4000 ms |

- To change a threshold, edit the number in the `assertions` block.
- To downgrade a hard failure to a warning while you investigate, change
  `"error"` to `"warn"` for that line.
- `minScore` is 0–1 (0.90 = a Lighthouse score of 90). `maxNumericValue` is in
  **milliseconds**.

**Two deliberate choices you should know about:**

1. **Desktop preset.** Lighthouse runs with `"preset": "desktop"`. Parcello
   targets desktop + Steam Deck (Chromium, ADR-0025), not phones, so desktop is
   the correct context — and CanvasKit Flutter Web realistically **cannot** hit
   Performance 90 under mobile throttling. If you truly need a mobile budget,
   add a second `collect`/`assert` context rather than switching this one.
2. **FCP/LCP are starting points.** The 3000/4000 ms thresholds were set without
   a local Lighthouse run (no Chrome in the authoring environment). **Calibrate
   on the first CI run:** open the Lighthouse report, read the real median FCP
   and LCP, and tighten each threshold to just above the green value. If the
   first run *fails* Performance ≥ 0.90, decide whether it is a real problem
   (e.g. compression not actually applied) or an over-strict budget for a
   CanvasKit app, and either fix the cause or relax the assertion.

## Running it locally

From `clients/flutter/`:

```sh
# 1. Build with source maps
flutter build web --release --source-maps

# 2. Size analysis  (needs the `brotli` CLI, or `pip install brotli`)
python3 tool/analyze_size.py build/web perf-report

# 3. Brotli size gate  (needs `brotli` and `jq`)
bash tool/size_budget.sh build/web perf-budgets.json

# 4. Lighthouse  (needs Node + Chrome)
npm install -g @lhci/cli http-server
lhci autorun          # reads lighthouserc.json
```

The size gate is the cheapest regression check and needs no browser — run it
whenever you touch dependencies, fonts, or `l10n`.

## When a budget fails

- **A size budget failed.** Look at `size-analysis.txt` to see which file grew.
  Common causes: a new dependency pulled into the first-load graph, a font added
  or un-subset, `flutter_localizations` gaining locales. If the growth is
  legitimate and unavoidable, raise the budget in the same PR with a note; if
  not, revert the cause.
- **Lighthouse failed.** Open the HTML report. Accessibility/Best-Practices
  failures are usually concrete and fixable (contrast, labels, console errors).
  Performance/FCP/LCP failures are usually the bundle growing or compression not
  being applied — cross-check with the size report.
