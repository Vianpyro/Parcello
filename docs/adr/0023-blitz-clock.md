# ADR-0023: blitz clock and the personal time bank (amends ADR-0015)

Status: accepted

## Context
`turn_seconds` (RoomSettings, ADR-0015) defaults to 25 via
`--turn-timeout`; an idle connected seat is auto-played the canonical
action at expiry. The v2 rhythm is harder: 12 seconds per decision,
softened by a personal reserve of 45 seconds for the whole match. The
engine has no clock (hard constraint) - this is all session layer.
Note: the v2 working draft said "set 12s in the mod"; timers are room
settings, not mod rules, so the change lands in the server defaults
instead.

## Decision
- The server default for `turn_seconds` becomes 12 (`--turn-timeout`
  default; hosts still edit per room, clamp 5..=3600 unchanged).
- New `RoomSettings.time_bank_seconds: Option<u64>` (default 45; 0/None
  disables), host-editable in the lobby like every other setting,
  bounded by `clamp_settings` to 0..=600.
- `room.rs` keeps one bank counter per seat beside `afk_deadline`. When
  a CONNECTED acting seat outlives `turn_seconds`, the deadline extends
  into their remaining bank and the overage drains it; the canonical
  auto-play fires only when the bank is dry. Any accepted command stops
  the drain and whatever remains is kept for later turns - one reservoir
  for the whole match, never refilled (v2 default).
- The disconnected path is unchanged: `DISCONNECTED_GRACE` (30s)
  applies and the bank does NOT - pulling the plug earns no extra time.
- The 5-second simultaneous windows (sealed bids ADR-0018, corruption
  votes ADR-0024) are separate, parallel timers; they consume neither
  the turn clock nor the banks.
- Clients see the bank: `Joined`/`GameStarted` carry the configured
  bank, `Update` carries the per-seat remaining vector - display data,
  the server owns the truth.
- Recorded but deferred: a chance card that refills the bank. Clean
  shape if wanted: the engine emits
  `Event::TimeBankRefill { player, amount }` and the SERVER applies it
  to its counter - the ADR-0010 clock/rule split. Not built until
  playtests ask for it.

## Consequences
- Server-only change plus protocol display fields; the engine is not
  touched.
- The room-task tests grow drain and hard-stop cases (tokio tests, like
  `game_started_carries_turn_seconds`).
- All three clients render the 12s countdown flowing into the bank
  drain (visual spec: `docs/visual-identity.md`, board screen).
- 12 seconds is only playable because every decision has a canonical
  default - each new v2 phase defines one (ADRs 0017/0018/0024).
