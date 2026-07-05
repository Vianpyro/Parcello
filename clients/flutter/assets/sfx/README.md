# Sound effects

Drop the sound effect files **in this folder** (`clients/flutter/assets/sfx/`),
with exactly these names:

| File | Played when |
| --- | --- |
| `dice-roll.mp3` | a player rolls the dice |
| `move-pawn-01.mp3` .. `move-pawn-11.mp3` | a pawn hops from tile to tile (one per step, cycling / random) |
| `stop-pawn-1.mp3` .. `stop-pawn-3.mp3` | a pawn lands on its destination (random) |
| `button-hover-0.mp3` .. `button-hover-9.mp3` | mouse hovers any button in the app (random) |
| `button-yes.mp3` | the affirmative choice in a confirm dialog (e.g. "Resign" in the resign prompt) |
| `button-no.mp3` | the negative choice in a confirm dialog (e.g. "Cancel" in the resign prompt) |
| `error.mp3` | the server rejects a command, or reports a connection error |
| `game-start.mp3` | the game starts (`game_started`) |
| `player-join.mp3` | a seat appears in the lobby (player or bot joins) |
| `player-leave.mp3` | a seat disappears from the lobby (player leaves, bot removed) |
| `timer-0.mp3` .. `timer-7.mp3` | a countdown milestone on the game or turn clock (60/30/10/5/4/3/2/1/0s remaining); cycled, no meaningful order |
| `toggle-on.mp3` / `toggle-off.mp3` | a lobby settings switch is flipped on/off ("Auction on decline") |

This directory is already registered in `pubspec.yaml` under
`flutter: assets:`, so any file you add here is bundled into the app.
Playback is already wired (`lib/sfx.dart`); just drop files with the exact
names above.
