# ADR-0005: rusqlite behind `GameHistory`, not SQLx

Status: accepted

## Context
The architecture doc names SQLite/SQLx behind Repository traits. `GameHistory`
is a fire-and-forget, append-only port called from room tasks; it needs
ordered durability, not async queries. SQLx brings a large async dependency
tree, longer builds, and a higher effective MSRV.

## Decision
`SqliteHistory` uses rusqlite (bundled SQLite) with one dedicated writer
thread owning the connection; trait methods enqueue over an mpsc channel and
never block on I/O. Writes are best-effort (logged on failure). Drop closes
the queue and joins the thread, draining pending records.

## Consequences
No async runtime coupling in persistence; room tasks never await disk.
`(seed, ordered command json)` rows are complete deterministic replays.
If richer queries are needed later (stats dashboards), a SQLx adapter can
replace this behind the same trait without touching callers.
