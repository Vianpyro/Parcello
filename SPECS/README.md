# SPECS - functional behaviour specifications

The layer that specifies WHAT each surface of Parcello DOES, observably -
its states, events, transitions, concurrency, the decisions it offers, and the
information that becomes primary - independent of any interface. A SPECS
document is the behaviour contract a surface must satisfy, whatever it is later
turned into.

This layer DEPENDS on DESIGN/ (it derives from it). DESIGN/ NEVER depends on
SPECS/. If a DESIGN document ever needed a SPECS document, the knowledge is in
the wrong place.

## Why SPECS exists

To capture the observable functional behaviour of a surface once, precisely, so
it can be reasoned about, validated, and reconstructed BEFORE and SEPARATELY
from how it is presented. The behaviour must be settled before the interface is
discussed.

## How SPECS differs from DESIGN

DESIGN owns WHY the experience feels as it does and in WHAT language it is
expressed - identity, colour meaning, type, motion doctrine, and the
cross-surface player behaviour and information needs (DESIGN/product/).

SPECS owns WHAT a specific surface DOES: its state machine, its transitions,
what may happen at the same time, which decisions are available in each state,
and which information becomes primary. DESIGN answers "why, and in what
language"; SPECS answers "what behaviour, exactly", per surface.

SPECS is the first DERIVED layer: it consumes DESIGN - above all
`DESIGN/product/PLAYER_BEHAVIOR` and `DESIGN/product/INFORMATION_ARCHITECTURE` -
and the game's rules, and turns them into a precise, testable behaviour
contract for one surface. It applies the cross-surface behaviour and information
model; it does not restate them.

## What a SPECS document owns

- The surface's PURPOSE and its behavioural promise to the player.
- Its complete set of STATES - modes and decision-contexts - and, per state:
  what triggers it, what ends it, which decisions are available, and which
  information becomes primary.
- Its TRANSITIONS: state A -> event -> state B, with interruptions, priorities,
  and conflicts.
- Its CONCURRENCY: what may happen simultaneously, what preempts or suspends
  what, what can never coexist - expressed as a compatibility matrix.
- The observable behavioural INVARIANTS of the surface (what must always, or
  must never, be possible).

## What a SPECS document NEVER owns

- No widgets, components, or design system.
- No layout, placement, regions, geometry, or pixels.
- No colours, typography, iconography, or any visual style.
- No animation, motion, timing, curves, or audio.
- No Flutter, framework, or any implementation detail.
- No feelings or emotional claims (DESIGN owns those).
- No rule DEFINITIONS (the rules and engine documents own those; SPECS
  describes only the observable behaviour that arises from them).
- No re-derivation of the cross-surface player behaviour or the information
  model (DESIGN/product/ owns those; SPECS references and applies them).
- No product priorities, roadmap, or balance.

## The convertibility test (the contract's teeth)

Every statement in a SPECS document must be honourable EQUALLY by a paper
wireframe and by a live interface. If a statement can only be satisfied by one
of them - because it assumes a control, a position, a colour, or a motion - it
does not belong in SPECS. A SPECS document that reads the same to a wireframe
sketcher and to an interface builder is correct; one that favours either is
leaking.

## Structure

- `game/` - the specifications of the in-game surfaces (the GameScreen first).

A SPECS document exists only for a surface that has behaviour worth specifying;
new documents are added when a real surface needs one, never in anticipation.
