# Upgrade plan

## Current state

Score: 8/10 (was 7/10 after pass 1, 6/10 before) — small, well-typed,
fail-closed money kernel with integer minor units, CI, property tests and a
float-free allocation primitive; invoice status is still an open string.

## Backlog

### P0
- (none open)

### P1
- Validate `InvoiceAggregate.status` against a closed set instead of any string.
  Blocked on the Finance API contract: the fixture only shows `paid`; get the
  full status list from the API owner before closing the set (an unknown value
  would then fail the shadow read closed).

### P2
- Consider a `Currency`-scale table test that pins ISO 4217 minor units.
- Add a `cargo deny`/`cargo audit` job once the dependency set grows.
- Document the `finance.v1` fixture contract in a short schema file.

## Done in pass 1
- Fixed: JSON number amounts were parsed via `f64` (exponent forms like `1e3`
  were silently accepted; 17+ significant digit decimals failed or could round).
  Enabled serde_json `arbitrary_precision` and added regression tests.
- Added `Money::to_decimal_string` with round-trip tests.
- Added unit tests for signs, per-currency scale, malformed input, overflow,
  non-positive postings, empty/mixed entries, blank IDs, balance currency
  mismatch, idempotency-key validation, and envelope precision.
- Added GitHub Actions CI (check script + MSRV 1.85 build).
- README: removed stale "no origin" statement; documented float-free guarantees.

## Done in this pass (pass 2)
- Decided: `Money::parse` rejects a trailing dot (`"1."`, `"-1."`); covered in
  the malformed-input test.
- Added `Money::allocate(&[u64])`: largest-remainder split on minor units with
  exact conservation, deterministic tie-break, sign symmetry, overflow-safe
  arithmetic, and `DomainError::InvalidAllocation` for an empty/zero or
  over-`u64::MAX` weight total.
- Added `tests/property_test.rs`: fixed-seed generated checks (no new
  dependencies) for parse/format round trip (20k cases), allocation
  conservation and per-part bounds (20k cases), and ledger balance
  conservation across 2k multi-posting entries.
