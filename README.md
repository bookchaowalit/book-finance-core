# Book Finance Core

`book-finance-core` is the first Rust boundary for the Finance Platform. It is
an intentionally small, pure typed kernel for money and double-entry ledger
invariants.

## Scope

- no database access;
- no network access;
- no credentials or environment-dependent configuration;
- no production API connection;
- no writes to the existing Solo Empire or BookTrading data stores.

The `finance.v1` summary fixture is sanitized contract input for the local
read-only shadow parser. It validates the typed read model and aggregate
invariants without opening the live API or any data store.

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

This is a local product scaffold under the Book Dev boundary. It has no
external `origin` yet and is intentionally not listed as an external GitHub
repository until a remote is deliberately created and registered. The root
repository must not treat this as a production cutover.

## Invariants covered

- currency-aware fixed-point money represented as integer minor units;
- explicit currency precision for THB, USD, EUR, GBP, JPY, BTC, and ETH;
- no cross-currency arithmetic;
- positive postings only;
- balanced single-currency ledger entries;
- idempotent transaction identifiers in a ledger book;
- deterministic account balance projection.
- typed `finance.v1` summary parsing with fail-closed contract drift checks.
- a feature-gated, non-money-moving reconciliation preview command.

## Verification

The repository owns its complete offline check; Solo Empire delegates to this
entrypoint without choosing Cargo flags or test subsets:

```bash
bash scripts/check.sh

# From the Solo Empire root, run the loopback HTTP parity lane:
task test:finance-shadow

# From the Solo Empire root, run the local command pilot:
task test:finance-command
```
