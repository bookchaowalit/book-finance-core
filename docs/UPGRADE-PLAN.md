# Upgrade plan

## Current state

Score: 8.5/10 (8 after pass 2, 7 after pass 1, 6 before) — small,
well-typed, fail-closed money kernel with integer minor units, CI, property
tests, a float-free allocation primitive and a schema-pinned `finance.v1`
contract; invoice status is still an open set.

## Backlog

### P0
- (none open)

### P1
- Validate `InvoiceAggregate.status` against a closed set instead of any string.
  Blocked on the Finance API contract: the fixture only shows `paid`; get the
  full status list from the API owner before closing the set (an unknown value
  would then fail the shadow read closed). Evidence so far (root repo scripts,
  no DB CHECK constraint): `draft`, `sent`, `overdue`, `paid`.

### P2
- Add a `cargo deny`/`cargo audit` job once the dependency set grows.
- Have the root repo's `task test:finance-shadow` also validate the live API
  response against `fixtures/finance-v1.schema.json`.

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

## Done in this pass (pass 3)
- Fixed: duplicate invoice `(status, currency)` groups were accepted (a
  consumer keying by them would double count or drop one); they now fail with
  `ShadowError::DuplicateInvoiceGroup`. Empty or whitespace-padded invoice
  statuses fail with `InvalidInvoiceStatus`.
- Fixed: a missing `period` key parsed as `None` (serde's implicit default for
  `Option`), hiding a renamed or dropped field; `period` is now required but
  nullable, matching the API, which always sends it.
- Added `fixtures/finance-v1.schema.json` (JSON Schema for the read contract)
  and `tests/contract_schema_test.rs`, which checks schema, fixture and parser
  field sets agree (every field required, unknown fields rejected, constants
  and currencies match the code). It found the `period` gap above.
- Added `tests/currency_scale_test.rs` pinning code and minor-unit scale for
  every currency (ISO 4217 for fiat; satoshi/wei for BTC/ETH), with an
  exhaustive match so a new variant cannot skip the table.
