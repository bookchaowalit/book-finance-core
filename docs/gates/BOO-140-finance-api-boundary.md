# BOO-140: finance API boundary gate evidence

Status: **evidence recorded. Owner decision PROPOSED, NOT ACCEPTED.** This
document does not approve a production cutover or a runtime move, and it does
not create `book-finance-api`.

## Gate

Next gate, verbatim from `repository-catalog/registries/system-repositories.yaml`
(solo-empire `63efeeb0`, system `finance`, line 228):

> Move the authenticated finance API boundary and keep the Rust kernel as a pure compatibility consumer.

Contract the gate answers: `bookchaowalit/architecture-docs`
`systems/architecture/solo-empire-api-contract-v1.md#bookfinance-api-v1`
(architecture-docs `ed94180`, section "BookFinance API v1", lines 82-109;
authentication classes at lines 55-63). The contract document's own
front matter is `status: retired` (line 8). It describes the old `infra/api`
server, which was archived on 2026-10-11.

Registry facts for this pair (`system-repositories.yaml` lines 205-228):
`data_owner` is "the ten finance cells ... and the CRM ledger for revenue and
invoices"; backend `finance-ledger-core` is `integration_status: kernel-only`;
the target `book-finance-api` is `remote_status: not-created`.

## Machine-readable evidence

- `fixtures/gates/finance-api-boundary.json`: the endpoint and auth-class map,
  the kernel purity policy, the full public kernel API, and its consumers.
- `tests/kernel_purity_test.rs`: the purity guard (run by `cargo test` in
  `scripts/check.sh`, so CI runs it).
- `tests/api_boundary_compat_test.rs`: the compatibility fixture tests.

## Where the boundary lives today

Sources inspected at `origin/main` of each sibling repository (read-only):

| Repository | Commit | What it is |
|---|---|---|
| `platform-legacy-api-archive` | `74b6844` | Archived `infra/api` server |
| `finance-accounts-api` | `ca43889` | cellkit cell `finance-accounts` |
| `finance-transactions-api` | `5952d22` | cellkit cell `transactions` |
| `finance-investments-api` | `d30f07b` | cellkit cell `investments` |
| `finance-reconciliations-api` | `7327586` | cellkit cell `reconciliations` |
| `finance-liabilities-api` | `91a992d` | cellkit cell `liabilities` |
| `sales-crm` (private) | `c15ce89` | `services/crm-api` with the CRM ledger |
| `finance-portfolio-web` | `be6e370` | the frontend consumer |

Key facts:

1. **Nothing serves `/api/finance/v1` anymore.** The archived server answers
   every `GET /api/finance/v1[/*]` with `410 Gone` "finance moved to the CRM
   ledger" before any auth check (`platform-legacy-api-archive/server.ts:553-556`).
   Its route matrix lists both routes as `public`
   (`tests/route_access_matrix_test.ts:26-27`).
2. **The frontend still calls it.** `finance-portfolio-web/src/lib/finance.ts:390-397`
   reads `FINANCE_API_BASE_URL` (default `http://127.0.0.1:3001`) and a
   `finance:read` bearer token, and lines 474-481 call all eight data endpoints.
3. **The data now lives in the cells and the CRM ledger.** All ten cells share an
   identical `src/cellkit/app.py` (same hash in every repository). It generates
   `GET /v1/<plural>`, `/stats`, `/legacy/{id}`, `/{id}` and write routes
   (`app.py:377-493`). Every list read calls the platform `allow(...,
   "<cell>.<resource>:read")` (`app.py:380`, `216-223`) after RS256 JWT
   verification (`app.py:193-198`, `auth.py:1`). The CRM ledger exposes
   `GET /api/v1/crm/ledger/{revenue,invoices,clients}`
   (`sales-crm services/crm-api/server.py:813-814`, `ledger.py:33,96-107`)
   behind one static `CRM_API_TOKEN` bearer (`server.py:146-153,806-812`).
4. **The kernel owns no endpoint.** It knows the read path as a constant
   (`src/lib.rs:11-12`), the summary producer name `book-finance-api` and the
   source of truth `sqlite.finance_control_plane` (`src/shadow.rs:6-7`). It
   validates a summary from a string (`src/shadow.rs:181`). Its only runtime
   inputs are stdin and one exact opt-in flag
   (`src/bin/finance-command-pilot.rs:9`, `src/command.rs:13,166`).

## Endpoint map: current owner and proposed target

All rows have auth class `bearer-finance-read`. Rows marked "orphaned" have no
live owner.

| Contract endpoint | Current owner (source route, permission) | Proposed target boundary | Kernel role |
|---|---|---|---|
| `GET /api/finance/v1` | Orphaned: archive returns 410 | `book-finance-api` discovery document | none |
| `GET /api/finance/v1/summary` | Split, no aggregator: `finance-transactions-api` `/v1/transactions` (`transactions.transaction:read`); `sales-crm` `/api/v1/crm/ledger/revenue` and `/invoices` (`CRM_API_TOKEN`); `finance-accounts-api` `/v1/accounts?status=active` (`finance-accounts.account:read`); `finance-liabilities-api` `/v1/liabilities` (`liabilities.liability:read`) | `book-finance-api` aggregates the sources, then the kernel validates the summary before it is served | `FinanceV1Summary::from_json` (library or `finance-v1-shadow` stdin) |
| `GET /api/finance/v1/transactions` | `finance-transactions-api` `/v1/transactions` (`transactions.transaction:read`) | proxy with contract query mapping and field allowlist | none |
| `GET /api/finance/v1/accounts` | `finance-accounts-api` `/v1/accounts` (`finance-accounts.account:read`) | proxy; strip `metadata`, `notes` | none |
| `GET /api/finance/v1/investments` | `finance-investments-api` `/v1/investments` (`investments.investment:read`) | proxy; strip `metadata`, `notes` | none |
| `GET /api/finance/v1/reconciliations` | `finance-reconciliations-api` `/v1/reconciliations` (`reconciliations.reconciliation:read`) | proxy; strip source references, notes | none |
| `GET /api/finance/v1/valuations` | `finance-investments-api` `/v1/valuations` and `/v1/market-prices` (`investments.valuation:read`, `investments.market-price:read`). The cell has no `latest`, `status` or `currency` filter. | facade computes the latest snapshot per position | none |
| `GET /api/finance/v1/risk-readiness` | Orphaned: derived from investments valuations and market prices. The FX snapshot contract has no cell. | facade computes the fail-closed readiness gate | none |
| `GET /api/finance/v1/invoices` | `sales-crm` `/api/v1/crm/ledger/invoices` (`CRM_API_TOKEN`) | server-side proxy; the CRM token never leaves the facade | none |

These cells have no endpoint in the v1 contract: `finance-budgets-api`,
`finance-cash-flow-api`, `finance-business-assets-api`,
`finance-subscriptions-api` and `finance-tax-api`. Exposing any of them through
the facade needs a contract update first.

## Auth-class map

| Contract class | Contract control | Current controls | Proposed target | Kernel |
|---|---|---|---|---|
| Bearer protected (`finance:read`) | Bearer token with `finance:read` scope, hashed scoped tokens | Archive: none, 410 before auth. Cells: platform RS256 JWT plus per-read platform authz decision. CRM: one shared static bearer. | `book-finance-api` verifies the client's `finance:read` token, calls the cells with a platform-issued token and the CRM ledger server-side, and never forwards client tokens | none |
| Public read | CORS allowlist | not used by finance v1 | not applicable | none |
| Signed webhook | provider signature | not used by finance v1 | not applicable | none |

## Kernel purity guard (cargo test)

`tests/kernel_purity_test.rs` reads `purity_policy` from the fixture and fails
in these cases:

- **Manifest drift.** `Cargo.toml` `[dependencies]` is anything other than
  `serde`, `serde_json`; any `[dev-dependencies]` or `[build-dependencies]`
  appear (target-specific and `[dependencies.x]` table forms included); or
  `build.rs`, `[package].build` or `links` appear.
- **Lockfile drift.** `Cargo.lock` resolves a crate outside the 9 allowlisted
  crates, or uses a non-crates.io source.
- **Library source.** Library files (`src/*.rs`, comments and string literals
  ignored) reach any of these:
  - `std::{net, fs, env, process, os, io, thread, time}`, including grouped
    imports like `use std::{fmt, fs}`;
  - `env!` or `option_env!`;
  - `extern`, or an alias of `std`;
  - any network, HTTP, DB, auth, env-loader or OS crate name, such as `tokio`,
    `reqwest`, `hyper`, `sqlx`, `diesel`, `rusqlite`, `postgres`,
    `jsonwebtoken`, `oauth2` or `dotenv` (full list in the fixture);
  - the types `TcpStream`, `UdpSocket`, `OpenOptions`, `SystemTime` or
    `Instant`.
- **Binary adapters.** A file under `src/bin/` is not declared in the fixture,
  uses more than stdio and exit codes, or reads any environment variable other
  than `env::var(FINANCE_CORE_COMMANDS_ENABLED_ENV)` in
  `finance-command-pilot`.

The guard also tests itself: it must flag 15 impure snippets and accept 7
harmless ones, such as a mention inside a comment or string, a lifetime, or a
field named `net`.

Before this change the existing source was checked and passes all of these
rules. The lockfile holds exactly `itoa, proc-macro2, quote, ryu, serde,
serde_derive, serde_json, syn, unicode-ident`.

## Compatibility fixture (cargo test)

`tests/api_boundary_compat_test.rs` runs five checks:

- `public_api` in the fixture must equal the extracted public items and
  inherent `pub` methods of `src/lib.rs`, `src/shadow.rs` and `src/command.rs`
  (63 entries).
- Each consumer's `uses` entry must be public API. Each in-repo consumer
  binary must still reference what it claims to use.
- Kernel constants must match the fixture: read path, schema, summary service
  and source of truth, command id, and opt-in flag.
- The endpoint map must cover exactly the contract's nine endpoints. All must
  be `GET` under `FINANCE_V1_READ_PATH`, with a known auth class, the target
  `book-finance-api`, and a recorded current owner. The kernel must never
  appear as an owner of an endpoint or an auth class.
- A consumer-shaped walk through the API (money, ledger, summary parse and
  parity, command preview) must compile and run with no I/O.

## Gaps

1. **No live `/api/finance/v1` owner.** `book-finance-api` is not created, the
   archive returns 410, and `finance-portfolio-web` still targets the retired
   facade. The verified-consumer frontend cannot get data from live systems
   today.
2. **Three auth models.** Contract hashed `finance:read` tokens, cell platform
   JWT plus authz, and a CRM shared static token do not line up. No delegation
   or on-behalf-of design exists for a facade calling cells.
3. **Money representation.** Cell amounts are `real` (float)
   (`finance-investments-api/cell.yaml:3`, and the same for every kit cell).
   The kernel requires exact decimal literals and rejects exponent notation. A
   facade that serialises cell floats can produce values the kernel rejects,
   or that are silently rounded.
4. **Stale kernel constant.** `FINANCE_V1_SOURCE_OF_TRUTH =
   "sqlite.finance_control_plane"` (`src/shadow.rs:7`) names retired storage.
   Changing it breaks every summary producer, so it is left unchanged here and
   needs a versioned contract decision.
5. **Query and limit drift.** Examples:
   - the contract uses `from`/`to` where the CRM ledger uses `since`/`until`;
   - contract `limit` is 1..100, while cells allow up to 1000
     (`app.py:54`) and the CRM ledger up to 500 (`ledger.py:34`);
   - valuations `latest`/`status`/`currency` and investments `currency` are
     not cell filters.
6. **Currency domain.** The kernel accepts 7 currencies. Cells accept any text
   up to 12 characters, and the CRM ledger accepts any 3 letters.
7. **Unowned derived data.** Risk-readiness and the FX snapshot contract have
   no owning cell.
8. **Lost operational controls.** The retired facade's audit log, rate limit
   and `X-Request-ID` controls are gone. Cells emit `X-Correlation-Id`
   (`app.py:181-191`) and platform audit, but nothing provides the
   contract-level guarantees.
9. **Retired contract.** The named contract is marked retired. The owner must
   decide whether `bookfinance-api-v1` is re-adopted as the target contract or
   superseded.
10. **Shared-infrastructure branch.** No `codex/shared-infrastructure-20261015`
    PR exists in this repository (`gh pr diff 1` returns no PR), so there is
    nothing to merge around.

## Owner decision — PROPOSED, NOT ACCEPTED

The owner must accept or reject each item below. Nothing here has been
accepted.

1. **Facade ownership.** `book-finance-api` becomes the only owner of
   `/api/finance/v1/*` and the `finance:read` class. It is a read-only (`GET`)
   aggregation facade over the five mapped cells and the CRM ledger. The cells
   and the CRM ledger stay the data owners, and writes stay in their own APIs.
2. **Kernel role.** `finance-ledger-core` stays a pure compatibility consumer:
   `serde` and `serde_json` only, no HTTP, DB, auth, environment or filesystem
   at runtime. The guard in this PR enforces this. The facade uses it to
   validate every summary before serving it, as a library if the facade is
   Rust, otherwise through the `finance-v1-shadow` stdin binary.
3. **Exact decimals.** Amounts cross the facade boundary as exact decimal
   strings or minor units, never as floats. Converting the cells' `real`
   columns is a cell-side change with its own gate.
4. **Versioned source of truth.** Replacing `source_of_truth` (and later
   `service`) in `finance.v1` requires a versioned contract (`finance.v1.1` or
   `finance.v2`) with a compatibility window. Until then the kernel keeps
   accepting only the v1 values.
5. **Auth design first.** The cell delegation and on-behalf-of token design and
   the CRM token handling are agreed before any facade code exists.
6. **Order of work.** Only after 1-5 are accepted: create the
   `book-finance-api` repository, then repoint `finance-portfolio-web`, then
   update the registry status. Each step is a separate, human-approved change.

## Not done in this change

- No owner decision has been recorded or accepted.
- No repository was created; `book-finance-api` remains `not-created`.
- No runtime move, deployment or frontend repoint.
- No change to any sibling repository or to the registry.
- The kernel constants are unchanged.
