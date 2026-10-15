# Book Finance Core

`book-finance-core` is the first Rust boundary for the Finance Platform. It is
an intentionally small, pure typed kernel for money and double-entry ledger
invariants.

## Scope

- no database access;
- no network access;
- no credentials; the only environment input is the exact opt-in flag
  `FINANCE_CORE_COMMANDS_ENABLED=true` read by the `finance-command-pilot`
  binary (any other value keeps the command disabled);
- no production API connection;
- no writes to the existing Solo Empire or BookTrading data stores.

The `finance.v1` summary fixture is sanitized contract input for the local
read-only shadow parser. It validates the typed read model and aggregate
invariants without opening the live API or any data store. The contract is
documented as JSON Schema in `fixtures/finance-v1.schema.json` (fields, types,
currencies, amount syntax, and the cross-field rules in its description);
`tests/contract_schema_test.rs` fails if the schema, the fixture and the
parser's accepted fields drift apart. Every field is required (`period` may be
`null` but not missing), invoice `(status, currency)` groups must be unique,
and invoice status must be a non-empty token.

The root repository adds a loopback-only harness with `task test:finance-shadow`:
it starts the current API against synthetic temporary data and pipes the JSON
response into the `finance-v1-shadow` binary. It does not contact an external
provider or use production credentials.

The local migration pilot also includes the feature-gated
`finance.reconciliation.preview` command. `task test:finance-command` verifies
its sanitized `finance.v1` input, preview-only output, exact opt-in flag, and
in-process idempotency behavior. This command is not an API route and cannot
write a ledger or Finance record.

The root repository separately exercises command-boundary readiness with
`task test:finance-command-boundary`. That harness is outside this nested
product repository and uses an explicit isolated state database for hashed
idempotency and redacted audit evidence only.

## Current status

This is a product scaffold under the Book Dev boundary, hosted at
`github.com/bookchaowalit/finance-ledger-core`. GitHub Actions runs
`scripts/check.sh` (tests, rustfmt, clippy with `-D warnings`) plus a build on
the declared `rust-version`. The root repository must not treat this as a
production cutover.

## Invariants covered

- currency-aware fixed-point money represented as integer minor units
  (`i128`); no floating-point type is used for amounts anywhere;
- JSON number amounts are read from their literal text (serde_json
  `arbitrary_precision`), so they never round-trip through `f64`; exponent
  notation such as `1e3` fails closed;
- lossless decimal formatting via `Money::to_decimal_string`;
- canonical amount text only: a trailing dot such as `1.` is rejected;
- float-free splitting via `Money::allocate(&weights)` (largest remainder:
  parts sum exactly to the original, each within one minor unit of its
  proportional share, zero weights get zero);
- explicit currency precision for THB, USD, EUR, GBP, JPY, BTC, and ETH;
- no cross-currency arithmetic;
- positive postings only;
- balanced single-currency ledger entries;
- idempotent transaction identifiers in a ledger book;
- deterministic account balance projection;
- deterministic property tests (`tests/property_test.rs`, fixed seed, no extra
  dependencies) for parse/format round trips, allocation conservation, and
  ledger balance conservation;
- typed `finance.v1` summary parsing with fail-closed contract drift checks;
- pinned minor-unit scale per currency (`tests/currency_scale_test.rs`).
- a feature-gated, non-money-moving reconciliation preview command.

## Verification

`Cargo.lock` is committed so local checks and CI use the same dependency
resolution. Run tests and lint with `--locked`; update the lockfile deliberately
with a dependency change, review its diff, and verify both stable Rust and the
declared minimum supported Rust version before merging it.

The repository owns its complete offline check; Solo Empire delegates to this
entrypoint without choosing Cargo flags or test subsets:

```bash
bash scripts/check.sh

# From the Solo Empire root, run the loopback HTTP parity lane:
task test:finance-shadow

# From the Solo Empire root, run the local command pilot:
task test:finance-command
```
