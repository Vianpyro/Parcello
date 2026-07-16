# Bundled fonts

The three typefaces from `docs/visual-identity.md` (Typography), bundled as
assets so the desktop/Steam client works fully offline - no Google Fonts
fetching at runtime (mandated by the design doc). All three are SIL Open Font
License 1.1; the licence text ships next to each file (`*-OFL.txt`) and is
also surfaced in-app via `LicenseRegistry` (Flutter's `showLicensePage`).

| Family         | Role (visual-identity.md)      | File               |
| -------------- | ------------------------------ | ------------------ |
| Inter          | Body / UI (400/500/700)        | `Inter.ttf`        |
| Fraunces       | Display / wordmark (700)       | `Fraunces.ttf`     |
| Source Serif 4 | Tile labels on the board       | `SourceSerif4.ttf` |

All three are **variable** fonts (a single file spans every weight/axis the
UI asks for via `fontWeight`), so one asset per family covers the whole range.

## Provenance

Downloaded verbatim from the official `google/fonts` repository (the upstream
OFL sources), `main` branch, then renamed to plain ASCII filenames:

- Inter          <https://github.com/google/fonts/tree/main/ofl/inter>
- Fraunces       <https://github.com/google/fonts/tree/main/ofl/fraunces>
- Source Serif 4 <https://github.com/google/fonts/tree/main/ofl/sourceserif4>

Integrity is pinned in `SHA256SUMS` (in this folder). To re-fetch and verify:

```sh
base=https://raw.githubusercontent.com/google/fonts/main/ofl
curl -fsSL -o Inter.ttf        "$base/inter/Inter%5Bopsz,wght%5D.ttf"
curl -fsSL -o Fraunces.ttf     "$base/fraunces/Fraunces%5BSOFT,WONK,opsz,wght%5D.ttf"
curl -fsSL -o SourceSerif4.ttf "$base/sourceserif4/SourceSerif4%5Bopsz,wght%5D.ttf"
sha256sum -c SHA256SUMS
```

Upstream may advance these variable fonts; re-pin `SHA256SUMS` deliberately
(a reviewed act) if you intentionally update a file.
