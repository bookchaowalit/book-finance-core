//! BOO-140 compatibility fixture: the kernel's public API as its consumers use
//! it, and the bookfinance-api-v1 boundary map the kernel stays behind.
//!
//! `fixtures/gates/finance-api-boundary.json` records which repository owns
//! each contract endpoint today, the proposed target boundary, and every
//! public kernel item. These tests fail when the source and the fixture drift.

use book_finance_core::command::{
    CommandEnvelope, CommandError, CommandExecutor, FINANCE_COMMAND_SCHEMA_VERSION,
    FINANCE_CORE_COMMANDS_ENABLED_ENV, IdempotencyKey, RECONCILIATION_PREVIEW_COMMAND,
    ReconciliationPreview, feature_flag_enabled,
};
use book_finance_core::shadow::{
    FINANCE_V1_SERVICE, FINANCE_V1_SOURCE_OF_TRUTH, FinanceV1Summary, InvoiceAggregate,
    LiabilityAggregate, RevenueAggregate, ShadowError, ShadowParity, TransactionAggregate,
};
use book_finance_core::{
    AccountId, Currency, Direction, DomainError, FINANCE_V1_READ_PATH, FINANCE_V1_SCHEMA_VERSION,
    LedgerBook, LedgerEntry, Money, Posting, TransactionId,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const SUMMARY: &str = include_str!("../fixtures/finance-v1-summary.json");

fn gate_fixture() -> Value {
    let text = fs::read_to_string(Path::new(ROOT).join("fixtures/gates/finance-api-boundary.json"))
        .expect("gate fixture is readable");
    serde_json::from_str(&text).expect("gate fixture is valid JSON")
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_owned())
        .collect()
}

/// Public items (`pub fn|struct|enum|const|static|type|trait|mod`) at module
/// level and `pub` methods of inherent impls, as `module::Item` or
/// `module::Type::method`. Restricted visibility (`pub(crate)`) is excluded.
fn public_api() -> BTreeSet<String> {
    let mut api = BTreeSet::new();
    for (file, module) in [
        ("src/lib.rs", "book_finance_core"),
        ("src/shadow.rs", "book_finance_core::shadow"),
        ("src/command.rs", "book_finance_core::command"),
    ] {
        let source = fs::read_to_string(Path::new(ROOT).join(file)).expect("kernel source");
        let mut impl_type: Option<String> = None;
        for line in source.lines() {
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            if line.starts_with("impl") {
                let head = line.trim_start_matches("impl").trim_start();
                let head = if head.starts_with('<') {
                    head.split_once('>')
                        .map_or(head, |(_, rest)| rest)
                        .trim_start()
                } else {
                    head
                };
                let target = head.split(" for ").last().unwrap_or(head);
                let is_trait_impl = head.contains(" for ");
                let name: String = target
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                impl_type = (!is_trait_impl).then_some(name);
                continue;
            }
            if line == "}" {
                impl_type = None;
                continue;
            }
            let (prefix, rest) = if let Some(rest) = line.strip_prefix("pub ") {
                (module.to_owned(), rest)
            } else if let (Some(ty), Some(rest)) = (&impl_type, line.strip_prefix("    pub ")) {
                (format!("{module}::{ty}"), rest)
            } else {
                continue;
            };
            let rest = rest.strip_prefix("const fn ").map_or(rest, |r| r);
            let rest = [
                "fn ", "struct ", "enum ", "const ", "static ", "type ", "trait ", "mod ",
            ]
            .iter()
            .find_map(|kind| rest.strip_prefix(kind))
            .unwrap_or(rest);
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                api.insert(format!("{prefix}::{name}"));
            }
        }
    }
    api
}

#[test]
fn fixture_lists_exactly_the_public_kernel_api() {
    let fixture = gate_fixture();
    let declared = strings(&fixture["public_api"]);
    let actual = public_api();
    let added: Vec<_> = actual.difference(&declared).collect();
    let removed: Vec<_> = declared.difference(&actual).collect();
    assert!(
        added.is_empty() && removed.is_empty(),
        "public kernel API drifted from fixtures/gates/finance-api-boundary.json: \
         added {added:?}, removed {removed:?}; update the fixture and the consumer list deliberately"
    );
}

#[test]
fn every_consumer_use_is_a_real_public_item() {
    let fixture = gate_fixture();
    let api = strings(&fixture["public_api"]);
    for consumer in fixture["consumers"].as_array().expect("consumers") {
        for used in strings(&consumer["uses"]) {
            assert!(
                api.contains(&used),
                "{} uses {used}, which is not public kernel API",
                consumer["name"]
            );
        }
        if let Some(path) = consumer["path"].as_str() {
            let source = fs::read_to_string(Path::new(ROOT).join(path)).expect("consumer source");
            // Whole identifiers only, so `parity` does not match `ShadowParity`.
            let identifiers: BTreeSet<&str> = source
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .collect();
            for used in strings(&consumer["uses"]) {
                let leaf = used.rsplit("::").next().expect("item name");
                assert!(
                    identifiers.contains(leaf),
                    "{path} no longer uses {used}; update the consumer fixture"
                );
            }
        }
    }
}

#[test]
fn kernel_constants_match_the_boundary_fixture() {
    let kernel = &gate_fixture()["kernel"];
    assert_eq!(kernel["read_path"], FINANCE_V1_READ_PATH);
    assert_eq!(kernel["schema_version"], FINANCE_V1_SCHEMA_VERSION);
    assert_eq!(kernel["summary_service"], FINANCE_V1_SERVICE);
    assert_eq!(
        kernel["summary_source_of_truth"],
        FINANCE_V1_SOURCE_OF_TRUTH
    );
    assert_eq!(
        kernel["command_schema_version"],
        FINANCE_COMMAND_SCHEMA_VERSION
    );
    assert_eq!(kernel["command"], RECONCILIATION_PREVIEW_COMMAND);
    assert_eq!(kernel["opt_in_env"], FINANCE_CORE_COMMANDS_ENABLED_ENV);
    let adapters = &gate_fixture()["purity_policy"]["binary_adapters"];
    assert_eq!(
        adapters["src/bin/finance-command-pilot.rs"]["env_var"],
        "FINANCE_CORE_COMMANDS_ENABLED_ENV"
    );
}

#[test]
fn boundary_map_covers_every_contract_endpoint_as_a_read_only_facade() {
    let fixture = gate_fixture();
    let target = fixture["target_backend"]["repository"]
        .as_str()
        .expect("target");
    assert_eq!(
        target, FINANCE_V1_SERVICE,
        "summary service names the target backend"
    );
    let auth_classes: BTreeSet<String> = fixture["auth_classes"]
        .as_array()
        .expect("auth classes")
        .iter()
        .map(|class| class["id"].as_str().expect("id").to_owned())
        .collect();
    let mut paths = BTreeSet::new();
    for endpoint in fixture["endpoints"].as_array().expect("endpoints") {
        let path = endpoint["path"].as_str().expect("path");
        assert!(paths.insert(path.to_owned()), "duplicate endpoint {path}");
        assert!(
            path == FINANCE_V1_READ_PATH || path.starts_with(&format!("{FINANCE_V1_READ_PATH}/")),
            "{path} is outside {FINANCE_V1_READ_PATH}"
        );
        assert_eq!(
            endpoint["method"], "GET",
            "{path}: the v1 facade is read-only"
        );
        let class = endpoint["auth_class"].as_str().expect("auth_class");
        assert!(
            auth_classes.contains(class),
            "{path}: unknown auth class {class}"
        );
        assert_eq!(endpoint["target_boundary"]["repository"], target, "{path}");
        assert!(
            endpoint["current_owner"]["status"].is_string()
                && endpoint["current_owner"]["sources"].is_array(),
            "{path}: current owner must be recorded"
        );
        for source in endpoint["current_owner"]["sources"]
            .as_array()
            .expect("sources")
        {
            assert!(
                source["repository"].is_string() && source["route"].is_string(),
                "{path}: every source names a repository and route"
            );
        }
    }
    assert_eq!(
        paths,
        strings(&fixture["contract"]["endpoints"]),
        "boundary map must cover exactly the contract's endpoint list"
    );
    let summary = fixture["endpoints"]
        .as_array()
        .expect("endpoints")
        .iter()
        .find(|endpoint| endpoint["path"] == format!("{FINANCE_V1_READ_PATH}/summary"))
        .expect("summary endpoint");
    assert_eq!(
        summary["kernel_consumer"],
        "book_finance_core::shadow::FinanceV1Summary::from_json"
    );
}

#[test]
fn kernel_never_owns_an_endpoint_or_auth_class() {
    let fixture = gate_fixture();
    for endpoint in fixture["endpoints"].as_array().expect("endpoints") {
        assert_ne!(
            endpoint["target_boundary"]["repository"],
            "finance-ledger-core"
        );
        for source in endpoint["current_owner"]["sources"]
            .as_array()
            .expect("sources")
        {
            assert_ne!(source["repository"], "finance-ledger-core");
        }
    }
    for class in fixture["auth_classes"].as_array().expect("auth classes") {
        assert_eq!(class["kernel_role"], "none", "{}", class["id"]);
    }
}

// A consumer-shaped walk through the public API: what book-finance-api (or
// the stdin binaries) would call. It only compiles while these signatures hold.
#[test]
fn consumer_uses_the_public_api_without_io() -> Result<(), Box<dyn std::error::Error>> {
    let thb: Currency = "THB".parse()?;
    assert_eq!((thb.code(), thb.scale()), ("THB", 2));
    let amount = Money::parse(thb, "1000.00")?;
    let fee = Money::new(thb, 2_500);
    assert_eq!(amount.checked_sub(fee)?.to_decimal_string(), "975.00");
    assert_eq!(amount.checked_add(fee)?.minor_units(), 102_500);
    assert!(Money::new(thb, 0).is_zero());
    assert_eq!(fee.checked_neg()?.currency(), thb);
    let split = amount.allocate(&[1, 1, 1])?;
    assert_eq!(split.iter().map(|m| m.minor_units()).sum::<i128>(), 100_000);

    let cash = AccountId::new("cash")?;
    let revenue = AccountId::new("revenue")?;
    let entry = LedgerEntry::try_new(
        TransactionId::new("txn-0001")?,
        [
            Posting::debit(cash.clone(), amount)?,
            Posting::credit(revenue.clone(), amount)?,
        ],
    )?;
    assert_eq!(entry.transaction_id().as_str(), "txn-0001");
    assert_eq!(entry.postings()[0].direction(), Direction::Debit);
    assert_eq!(entry.postings()[0].account_id().as_str(), "cash");
    assert_eq!(entry.postings()[1].amount(), amount);
    let mut book = LedgerBook::default();
    book.post(entry.clone())?;
    assert!(matches!(
        book.post(entry),
        Err(DomainError::DuplicateTransaction(_))
    ));
    assert_eq!(book.entries().len(), 1);
    assert_eq!(book.account_balance(&cash, thb)?.minor_units(), 100_000);

    let summary = FinanceV1Summary::from_json(SUMMARY)?;
    let parity: ShadowParity = summary.parity();
    assert_eq!(parity.service, FINANCE_V1_SERVICE);
    let _: (
        &TransactionAggregate,
        &RevenueAggregate,
        &InvoiceAggregate,
        &LiabilityAggregate,
    ) = (
        &summary.transactions[0],
        &summary.revenue[0],
        &summary.invoices[0],
        &summary.liabilities[0],
    );
    assert!(matches!(
        FinanceV1Summary::from_json("{}"),
        Err(ShadowError::Json(_))
    ));

    assert!(!feature_flag_enabled(None));
    let key = IdempotencyKey::new("preview-compat-001")?;
    let _ = key;
    let envelope = CommandEnvelope::from_json(&format!(
        r#"{{"command":"{RECONCILIATION_PREVIEW_COMMAND}","idempotency_key":"preview-compat-001","summary":{SUMMARY}}}"#
    ))?;
    let preview: ReconciliationPreview = CommandExecutor::default().execute(envelope)?;
    assert!(!preview.write_effects && !preview.ledger_mutation);
    assert!(matches!(
        IdempotencyKey::new(""),
        Err(CommandError::InvalidIdempotencyKey)
    ));
    Ok(())
}
