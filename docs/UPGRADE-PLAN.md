# Upgrade plan

## Current state

Score: 7/10 (was 6/10) — small, well-typed, fail-closed money kernel with
integer minor units; now has CI and edge-case coverage, but no property tests
or rounding/allocation primitives yet.

## Backlog

### P0
- (none open)

### P1
- Decide whether `Money::parse` should reject a trailing dot (`"1."`), which
  is currently accepted as `1.00`; document the choice in a test.
- Add property-based tests (e.g. `proptest`) for `parse`/`to_decimal_string`
  round-trip and ledger balance conservation across many entries.
- Add an explicit allocation/split primitive (largest-remainder) before any
  caller needs to divide money, so nobody reaches for floats.
- Validate `InvoiceAggregate.status` against a closed set instead of any string.

### P2
- Consider a `Currency`-scale table test that pins ISO 4217 minor units.
- Add a `cargo deny`/`cargo audit` job once the dependency set grows.
- Document the `finance.v1` fixture contract in a short schema file.

## Done in this pass
- Fixed: JSON number amounts were parsed via `f64` (exponent forms like `1e3`
  were silently accepted; 17+ significant digit decimals failed or could round).
  Enabled serde_json `arbitrary_precision` and added regression tests.
- Added `Money::to_decimal_string` with round-trip tests.
- Added unit tests for signs, per-currency scale, malformed input, overflow,
  non-positive postings, empty/mixed entries, blank IDs, balance currency
  mismatch, idempotency-key validation, and envelope precision.
- Added GitHub Actions CI (check script + MSRV 1.85 build).
- README: removed stale "no origin" statement; documented float-free guarantees.
