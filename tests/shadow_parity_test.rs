use book_finance_core::shadow::{
    FINANCE_V1_SERVICE, FINANCE_V1_SOURCE_OF_TRUTH, FinanceV1Summary, ShadowError, ShadowParity,
};
use book_finance_core::{Currency, FINANCE_V1_SCHEMA_VERSION};

const FINANCE_V1_SUMMARY: &str = include_str!("../fixtures/finance-v1-summary.json");

#[test]
fn parses_and_projects_the_sanitized_finance_v1_summary() {
    let summary =
        FinanceV1Summary::from_json(FINANCE_V1_SUMMARY).expect("valid finance.v1 fixture");

    assert_eq!(summary.schema_version, FINANCE_V1_SCHEMA_VERSION);
    assert_eq!(summary.service, FINANCE_V1_SERVICE);
    assert_eq!(summary.source_of_truth, FINANCE_V1_SOURCE_OF_TRUTH);
    assert_eq!(summary.transactions[0].currency, Currency::Thb);
    assert_eq!(summary.transactions[0].income.minor_units(), 100_000);
    assert_eq!(summary.transactions[0].expense.minor_units(), 25_000);
    assert_eq!(summary.transactions[0].net.minor_units(), 75_000);
    assert_eq!(summary.revenue[0].received.minor_units(), 100_000);
    assert_eq!(summary.liabilities[0].outstanding.minor_units(), 5_000);
    assert_eq!(summary.accounts_active, 1);
    assert_eq!(
        summary.parity(),
        ShadowParity {
            schema_version: FINANCE_V1_SCHEMA_VERSION,
            service: FINANCE_V1_SERVICE,
            period: Some("2026-08".to_owned()),
            transaction_groups: 1,
            revenue_groups: 1,
            invoice_groups: 1,
            active_accounts: 1,
            liability_groups: 1,
        }
    );
}

#[test]
fn rejects_transaction_net_drift_before_it_can_be_consumed() {
    let drifted = FINANCE_V1_SUMMARY.replace("\"net\": 750", "\"net\": 751");
    let error = FinanceV1Summary::from_json(&drifted).expect_err("net drift must fail closed");

    assert!(matches!(error, ShadowError::TransactionNetMismatch { .. }));
}

#[test]
fn rejects_unknown_fields_to_make_contract_drift_visible() {
    let drifted = FINANCE_V1_SUMMARY.replace(
        "  \"transactions\": [",
        "  \"unexpected_field\": true,\n  \"transactions\": [",
    );
    let error = FinanceV1Summary::from_json(&drifted).expect_err("unknown fields must fail closed");

    assert!(matches!(error, ShadowError::Json(_)));
}
