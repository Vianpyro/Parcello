# Room Settings - functional specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It describes WHAT the room's settings do, never how they are presented. Every
statement must be honourable equally by a paper wireframe and by a live
interface.

Room settings are the agreed configuration of a room, decided before play. This
document specifies their functional behaviour: what is configurable, who may
change it and when, how a change is validated and applied, and how it is frozen
into a game. It does NOT own the specific allowed ranges or the meaning of each
rule - those belong to the rules and engine documents; this document owns only
the BEHAVIOUR around them.

---

# Purpose

To let a room reach a single, valid, shared starting configuration that every
participant can see and that exactly one participant - the host - controls,
frozen into the game at launch so the whole game is played under one agreed set
of rules.

---

# Categories

The configuration has two families. The specific dimensions are named because
they define the functional surface; their numeric ranges and semantics are
owned elsewhere.

- **Clocks.** The overall game length, the per-turn limit, and the personal time
  reserve. Each clock has two functional states: OFF (absent - the game imposes
  no such limit) or a bounded duration.
- **Rules.** Grouped: starting economy (opening balance, pass-start income);
  movement (the velocity range - a minimum and a maximum that must exceed it);
  estate (maximum development level; the shared building-pool factors);
  win conditions (the victory-point target; the domination group count);
  aggression (the seizure premium; the rent-boost step); world (the spotlight
  strength and its duration); solvency (the bankruptcy threshold).

**Not configurable here.** The board / content set is chosen when the room is
created and is immutable afterwards - it is not a room-settings dimension. A
ranked room's configuration is fixed by the matchmaker and is not configurable
at all.

---

# Ownership and window

- Only the **host** (the room's first-seat occupant) may change settings.
- Only while the room is **assembling** (pre-game). Once the game is live,
  settings are frozen and no change is accepted.
- In a **ranked** room, settings are not editable by anyone.

A change from a non-host, a change once the game is live, and any change in a
ranked room are all refused with a reason and leave the settings untouched.

---

# Persistence

The room holds exactly one settings value, shared identically with every seated
participant and every observer. That value lives only for the room's assembling
life; it is copied into the game at launch and then no longer changes; it
disappears when the room dissolves. Settings do NOT persist across rooms or
across sessions: a new room begins from the server's configured defaults, never
from a previous room's choices. There is no per-participant saved configuration.

---

# Validation

Validation has two independent layers:

1. **At change time - coercion, never rejection.** Every field of a submitted
   configuration is pulled into its allowed range: an out-of-range value is
   silently coerced to the nearest bound, and cross-field constraints are
   auto-repaired the same way (the movement maximum is coerced to sit above the
   movement minimum). A submitted change is therefore ALWAYS accepted and the
   resulting value is always internally safe; the value the room ends up holding
   may differ from the value submitted, and the shared value that all
   participants see is the coerced one.
2. **At launch - a final gate that can refuse.** When the host starts the game,
   the game is built from the effective settings and content; if that
   combination is rejected, the launch fails and the room stays assembling with
   the reason. This is the only place an unacceptable configuration surfaces as
   a refusal rather than a coercion.

---

# Cancellation

There is no transactional edit session and no remembered rollback. Each accepted
change is a COMPLETE REPLACEMENT of the shared settings and takes effect on the
shared value immediately. Consequently:

- An edit that is composed but never submitted has no effect at all - the shared
  settings remain whatever was last accepted. "Cancelling" such an edit is a
  no-op by definition.
- To undo a change that WAS accepted, a further change re-submitting the
  previous values (or the defaults) is made; the room keeps no prior version to
  revert to.

---

# Immediate or deferred application

A change applies in three distinct senses, and the distinction is the core of
this subsystem:

- **Immediate** on the shared room configuration: an accepted change replaces
  the room's settings and is reflected to every participant at once, before any
  next action can depend on it.
- **Deferred and one-shot** on the actual game: settings affect a game only at
  launch, when they are copied into the game and its clocks. From that instant
  they are FROZEN - no later change ever reaches a live game.
- **Resolved-at-launch** for player-count-dependent effects: quantities that
  depend on the number of participants (the shared building pools) are not
  stored in the settings; they are computed at launch from the seat count and
  the relevant factors. A change to the roster therefore alters the launched
  game's derived quantities without any settings change.

---

# Conflict handling

- **Single writer.** Only the host edits, so two participants never change the
  settings at the same time.
- **Serial application.** Changes take effect in the order received; a change
  and a launch are ordered against each other - the launch freezes whatever was
  accepted last, and no change exists after a launch (launch ends editability).
- **Host migration.** If the host leaves, the new first-seat occupant inherits
  the current shared settings and may continue editing them; the settings value
  is unaffected by which participant holds authority.
- **Roster versus settings.** A change to the number of participants never
  invalidates the stored settings; it only changes the launch-resolved
  quantities (the pools).
- **Authority conflicts.** A change from a participant who is not, or is no
  longer, the host is refused; the shared value is unchanged.

---

# Invalid values

- An out-of-range field is coerced to the nearest bound at change time; it is
  never rejected and never leaves the room holding an out-of-range value.
- A cross-field violation (a maximum not above its minimum) is auto-repaired by
  coercion at change time.
- "Off" is a valid state for each clock, not an invalid value.
- The only configuration that is REFUSED rather than coerced is one the game
  rejects at launch (the launch gate); it fails the launch and keeps the room
  assembling, with the reason.

---

# Default values

A new room begins from the server's configured defaults: a default game length,
a default turn limit, a default time reserve, and the base content's rule
defaults. A clock's default may be OFF. Defaults are the server operator's
choice, not a participant's, and they are the starting point every fresh room
inherits.

---

# Reset

There is no dedicated reset operation. A room is returned to defaults by applying
the default configuration as an ordinary change - a full replacement with the
default values - subject to the same authority (host, while assembling) and the
same coercion as any other change. A ranked room, whose configuration is fixed,
cannot be reset.

---

# Functional Guarantees

- **One authoritative value.** At any instant the room holds exactly one
  settings value, identical for every participant.
- **Safe by coercion.** Every accepted value is internally valid; no input can
  make the room hold an unsafe or self-contradictory configuration.
- **Single authority and window.** Only the host, only while assembling; never a
  non-host, never once the game is live, never in a ranked room.
- **Freeze at launch.** The game is built from the settings exactly once, at
  launch, and never observes a later change.
- **Shared truth.** Every accepted change - including the coerced result of an
  out-of-range submission - is reflected to all participants before the next
  dependent action.
- **No hidden state.** The room remembers only the last accepted value: no
  draft, no history, no rollback it can restore.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any component, control, input, or navigation affordance;
- any layout, placement, region, geometry, or size;
- any colour, typography, iconography, or visual style;
- any animation, motion, timing curve, or sound;
- any framework or implementation detail;
- the specific numeric ranges of any field, or the meaning/effect of any rule
  (owned by the rules and engine documents; this document references that bounds
  and meanings exist, never their values);
- any feeling or emotional claim (owned by `DESIGN/PLAYER_EXPERIENCE`).

---

# Phase 4 - Self-critique

- **Two requested categories have no dedicated mechanic; specified truthfully.**
  "Cancellation" and "Reset" are NOT server operations in Parcello. There is no
  transactional edit with a cancel and no reset command: every change is a
  complete, immediate replacement, and "undoing" is re-applying prior or default
  values. Both categories are covered by the real behaviour and flagged here,
  because SPECS describes observable behaviour, not desired affordances. If a
  true draft/cancel or a one-touch reset is ever wanted, that is a product/rules
  change and only then a spec.
- **The subtle, easily-missed truth: validation coerces, it does not reject.** An
  out-of-range submission is SILENTLY changed to a bounded value (and the shared
  value everyone sees is the coerced one), rather than bounced. This has a real
  consequence any interface must honour - the value that becomes shared truth may
  differ from what was submitted - so it is stated in Validation, Invalid values,
  and the guarantees, not buried.
- **Three senses of "apply" separated.** Immediate (on the shared room value),
  deferred-and-frozen (on the game, once, at launch), and resolved-at-launch (the
  player-count-dependent pools, never stored). Collapsing these would make the
  spec wrong; they are the core of the subsystem and are kept distinct.
- **Ranked rooms are a read-only mode of the whole subsystem** - the matchmaker
  fixes the configuration and every host power is refused. Called out so it is
  not assumed that "host" always implies "may configure".
- **Boundary honesty (SPECS vs rules/engine).** The numeric ranges and the effect
  of each rule are RULES/engine knowledge; this document owns only the behaviour
  (coercion, freeze, authority, replacement) and deliberately does not enumerate
  the bounds. The one risk is drift: if the engine's bounds or the freeze rule
  change, this document's behavioural claims must be re-checked - which, like the
  other SPECS documents, makes it a candidate to be validated against the room's
  actual settings handling rather than trusted as prose.
- **Convertibility test applied.** No statement names a control, a position, a
  colour, or a motion; every claim is behavioural (who, when, coerce vs refuse,
  immediate vs frozen, one shared value).
