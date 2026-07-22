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
   `lighthouserc.json`. **Currently `warn`-level** during the calibration
   period (see below): breaches are printed and reported but do not fail the
   run. Scheduled to become a hard failure, same as Gate 1, once calibrated.
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
The five requested budgets, currently at **`warn`** level (see "Lighthouse
calibration period" below — this is deliberate and temporary, not a relaxed
target):

| Assertion | Budget | Level |
|---|---|---|
| `categories:performance` | ≥ 0.90 | `warn` |
| `categories:accessibility` | ≥ 0.95 | `warn` |
| `categories:best-practices` | ≥ 0.95 | `warn` |
| `first-contentful-paint` | ≤ 3000 ms | `warn` |
| `largest-contentful-paint` | ≤ 4000 ms | `warn` |

- To change a threshold's *value*, edit the number in the `assertions` block —
  independent of its level (`warn`/`error`).
- `minScore` is 0–1 (0.90 = a Lighthouse score of 90). `maxNumericValue` is in
  **milliseconds**.
- `["warn", {...}]` vs `["error", {...}]` is Lighthouse CI's own assertion
  level: `lhci autorun` exits non-zero (fails the job) only on an `error`-level
  breach; a `warn`-level breach is printed in the assertion table but the
  process still exits 0. That is the entire mechanism behind the calibration
  period — no workflow logic depends on it.

**One deliberate choice you should know about:** Lighthouse runs with
`"preset": "desktop"`. Parcello targets desktop + Steam Deck (Chromium,
ADR-0025), not phones, so desktop is the correct context — and CanvasKit
Flutter Web realistically **cannot** hit Performance 90 under mobile
throttling. If you truly need a mobile budget, add a second `collect`/`assert`
context rather than switching this one.

### Lighthouse calibration period (current state)

**All five Lighthouse assertions are `warn`, not `error`.** The deterministic
Brotli size gate (`tool/size_budget.sh`) is, for now, the **only** check in
`web-perf.yml` that can fail the workflow. This is intentional, not a
weakening of the pipeline:

- **Lighthouse has real run-to-run variance** — CPU/network jitter on the
  runner, GC pauses, font-loading races — that a single fixed threshold cannot
  absorb without either flaking on good builds or being loose enough to be
  meaningless. The FCP/LCP numbers here (3000 ms / 4000 ms) were set without a
  local Lighthouse run (no Chrome in the authoring environment) and have not
  yet been checked against real CI variance.
- The `numberOfRuns: 3` + `median-run` aggregation in `lighthouserc.json`
  already damps *within-run* noise; what calibration adds is *across-run*
  evidence — is the median itself stable build to build, or does it swing
  enough to false-positive an `error` gate on an unrelated PR?
- Promoting straight to `error` on day one risks the classic failure mode of a
  perf gate: it flakes once, someone merges with `--no-verify`-equivalent
  urgency, and the gate's credibility (and enforcement) never recovers.

**Promotion criteria:** once **several dozen consecutive CI runs on `main`**
have gone green on all five Lighthouse assertions at their current numeric
values — i.e. the thresholds hold up under real variance, not just one lucky
run — flip each assertion's level from `"warn"` to `"error"` in
`lighthouserc.json`. Use that same run history to tighten FCP/LCP toward the
observed median plus a small margin, the same ratchet spirit as the size
budgets above.

> **TODO(web-perf-calibration):** promote all five assertions in
> `clients/flutter/lighthouserc.json` from `"warn"` to `"error"` once dozens of
> consecutive green `main` runs support the current (or a tightened) set of
> numeric thresholds. Tracked inline in that file's header comment too —
> don't remove the comment until this promotion happens.

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

## Performance history

Every push to `main` that goes green also records a row of headline metrics
(Lighthouse Performance score, FCP, LCP, and the JS / Fonts / CanvasKit /
First-load-total Brotli sizes) so their evolution over time is visible, not
just the pass/fail snapshot in the current run's artifact.

**No database, no external service** - just this repo. The `record-history`
job in `web-perf.yml` (runs after `perf`, `main`-push only) appends one entry
to a JSON array and regenerates a static dashboard, both committed to a
dedicated **orphan branch, `perf-history`**, never merged into `main`:

- `perf-history.json` - the full history, one object per build:
  `date`, `sha`, `run_url`, `performance_score`, `fcp_ms`, `lcp_ms`,
  `js_brotli_kb`, `fonts_brotli_kb`, `canvaskit_brotli_kb`, `total_brotli_kb`.
- `index.html` - a self-contained dashboard (embeds the JSON, draws it with
  plain `<canvas>`, no chart library, no CDN) plotting each metric over time.

Why an orphan branch rather than a folder on `main` or an external
dashboard: the data commits are unrelated to the code history (one metrics
commit per build would otherwise pollute `git log`/`git blame` on `main`),
they need no code review, and a branch is something `git clone`/`git log
perf-history` can inspect with tools every contributor already has - no new
account, service, or credential.

**Reading it:**
- `git fetch origin perf-history && git show perf-history:perf-history.json`
  for the raw numbers.
- Check out the branch and open `index.html` locally in a browser
  (`git worktree add ../perf-history origin/perf-history`) - `fetch()`-free by
  design so a plain `file://` open works.
- Or enable **GitHub Pages** once, pointed at the `perf-history` branch's
  root, for a persistent URL - still entirely GitHub, no third-party host.

The generator is `clients/flutter/tool/record_perf_history.py`. It reads two
things the `perf` job already produces: `size-budget.json` (a small addition
to `tool/size_budget.sh`'s output, alongside the existing text table) and
`lighthouse/manifest.json` (lhci's filesystem upload picks the
"representative" - median - run itself, matching the `median-run`
aggregation the assertions already use). Re-running it is idempotent per
commit: a re-run for the same `sha` replaces that entry instead of
duplicating it.

## When a budget fails

- **A size budget failed.** This still **fails the workflow** (Gate 1 is
  unconditional). Look at `size-analysis.txt` to see which file grew. Common
  causes: a new dependency pulled into the first-load graph, a font added or
  un-subset, `flutter_localizations` gaining locales. If the growth is
  legitimate and unavoidable, raise the budget in the same PR with a note; if
  not, revert the cause.
- **A Lighthouse assertion breached its budget.** During the calibration period
  (above) this is a **warning, not a failure** — the workflow still goes green.
  It shows up as `warn` in the `lhci autorun` log output and in the uploaded
  `lighthouse/` report; don't ignore it just because it isn't red. Open the
  HTML report: Accessibility/Best-Practices breaches are usually concrete and
  fixable (contrast, labels, console errors); Performance/FCP/LCP breaches are
  usually the bundle growing or compression not being applied — cross-check
  with the size report. Once assertions are promoted to `error` (see above), a
  breach fails the workflow the same way a size budget does today.
