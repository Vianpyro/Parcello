# Sound effects

Drop the sound effect files **in this folder** (`clients/flutter/assets/sfx/`),
with exactly these names. A missing file is silent, never a crash - so a new
name below can be added at any time without touching code.

## Earcons (category sounds)

Sound is a property of a beat's **category**, not of the event that produced it
(`docs/motion-language.md` section 4): one identity per category, reused
everywhere, so a player absorbs the vocabulary without being taught it. These
four are the vocabulary; the rest of the table is UI chrome.

| File | Category | Status |
| --- | --- | --- |
| `card-draw.mp3` | a chance card is revealed | **missing** - silent today |
| `cash-gain.mp3` | money arrived **at you** | **missing** - silent today |
| `cash-loss.mp3` | money left **you** (a third party's money makes no sound: at six seats that is a wall of noise) | **missing** - silent today |
| `arrest.mp3` | P1 - the table stops (a bankruptcy, a win). One low, long tone; never layered | **missing** - silent today |

## The rest

| File | Played when |
| --- | --- |
| `dice-roll.mp3` | a player plays a movement card. **The file name is a leftover**: Parcello has had no dice since ADR-0017, and the clip is a stand-in the audio pass should replace (the code calls this `sfx.cardPlay()`) |
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
