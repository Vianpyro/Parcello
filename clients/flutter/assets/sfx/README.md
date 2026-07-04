# Sound effects

Drop the sound effect files **in this folder** (`clients/flutter/assets/sfx/`),
with exactly these names:

| File | Played when |
| --- | --- |
| `dice-roll.mp3` | a player rolls the dice |
| `move-pawn-01.mp3` .. `move-pawn-11.mp3` | a pawn hops from tile to tile (one per step, cycling / random) |
| `stop-pawn-1.mp3` .. `stop-pawn-3.mp3` | a pawn lands on its destination (random) |

This directory is already registered in `pubspec.yaml` under
`flutter: assets:`, so any file you add here is bundled into the app.

## To actually play them

Playback is not wired yet because the audio plugin needs a system setting
on this machine:

1. Enable **Windows Developer Mode** (native-plugin symlinks):
   `start ms-settings:developers`.
2. Add the dependency: `flutter pub add audioplayers`.
3. Ask me to wire the playback — the intended mapping is the table above:
   `dice-roll` on the `dice_rolled` event, a `move-pawn-NN` per hop during
   the board's pawn glide animation, and a random `stop-pawn-N` on landing.

Keep the exact filenames above; the wiring will reference them directly.
