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

fn summary_with_transaction(income: &str, expense: &str, net: &str) -> String {
    FINANCE_V1_SUMMARY
        .replace("\"income\": 1000", &format!("\"income\": {income}"))
        .replace("\"expense\": 250", &format!("\"expense\": {expense}"))
        .replace("\"net\": 750", &format!("\"net\": {net}"))
}

#[test]
fn json_number_amounts_keep_their_exact_decimal_text() {
    // 17 significant digits cannot round-trip through f64; the kernel must
    // parse the literal text rather than a float approximation.
    let input = summary_with_transaction("1234567890123456.78", "0.01", "1234567890123456.77");
    let summary = FinanceV1Summary::from_json(&input).expect("exact decimal literals parse");

    assert_eq!(
        summary.transactions[0].income.minor_units(),
        123_456_789_012_345_678
    );
    assert_eq!(summary.transactions[0].expense.minor_units(), 1);
    assert_eq!(
        summary.transactions[0].net.minor_units(),
        123_456_789_012_345_677
    );
}

#[test]
fn json_number_amounts_beyond_u64_stay_exact() {
    let input = summary_with_transaction("100000000000000000000", "0", "100000000000000000000");
    let summary = FinanceV1Summary::from_json(&input).expect("large integer literal parses");

    assert_eq!(
        summary.transactions[0].income.minor_units(),
        10_000_000_000_000_000_000_000
    );
}

#[test]
fn exponent_notation_amounts_fail_closed() {
    let input = summary_with_transaction("1e3", "250", "750");
    let error = FinanceV1Summary::from_json(&input).expect_err("exponent notation must fail");

    assert!(matches!(error, ShadowError::Domain(_)));
}

#[test]
fn decimal_string_amounts_are_accepted() {
    let input = summary_with_transaction("\"1000.50\"", "\"250.25\"", "\"750.25\"");
    let summary = FinanceV1Summary::from_json(&input).expect("decimal strings parse");

    assert_eq!(summary.transactions[0].net.minor_units(), 75_025);
}

#[test]
fn over_precise_amounts_are_rejected_for_the_currency_scale() {
    let input = summary_with_transaction("1000.001", "250", "750.001");
    let error = FinanceV1Summary::from_json(&input).expect_err("THB allows two decimals");

    assert!(matches!(error, ShadowError::Domain(_)));
}

const PAID_INVOICE: &str = "{\n      \"status\": \"paid\",\n      \"currency\": \"THB\",\n      \"count\": 1,\n      \"amount\": 1000\n    }";

fn summary_with_invoices(invoices: &[&str]) -> String {
    assert!(
        FINANCE_V1_SUMMARY.contains(PAID_INVOICE),
        "fixture invoice shape changed"
    );
    FINANCE_V1_SUMMARY.replace(PAID_INVOICE, &invoices.join(",\n    "))
}

fn invoice(status: &str, currency: &str) -> String {
    format!(
        "{{\"status\": \"{status}\", \"currency\": \"{currency}\", \"count\": 1, \"amount\": 10}}"
    )
}

#[test]
fn distinct_invoice_status_and_currency_groups_are_accepted() {
    let (paid_usd, open_thb) = (invoice("paid", "USD"), invoice("open", "THB"));
    let input = summary_with_invoices(&[PAID_INVOICE, &paid_usd, &open_thb]);
    let summary = FinanceV1Summary::from_json(&input).expect("distinct groups parse");

    assert_eq!(summary.parity().invoice_groups, 3);
}

#[test]
fn duplicate_invoice_groups_fail_closed() {
    let duplicate = invoice("paid", "thb");
    let input = summary_with_invoices(&[PAID_INVOICE, &duplicate]);
    let error = FinanceV1Summary::from_json(&input).expect_err("duplicate group must fail");

    assert!(matches!(
        error,
        ShadowError::DuplicateInvoiceGroup { ref status, currency: Currency::Thb } if status == "paid"
    ));
    assert_eq!(error.to_string(), "duplicate paid/THB group in invoices");
}

#[test]
fn blank_or_padded_invoice_status_fails_closed() {
    for status in ["", "   ", " paid", "paid "] {
        let row = invoice(status, "THB");
        let error = FinanceV1Summary::from_json(&summary_with_invoices(&[&row]))
            .expect_err("status must be a clean token");

        assert!(
            matches!(error, ShadowError::InvalidInvoiceStatus),
            "{status:?}"
        );
    }
}

#[test]
fn period_may_be_null_but_not_missing() {
    let null_period = FINANCE_V1_SUMMARY.replace("\"period\": \"2026-08\"", "\"period\": null");
    let summary = FinanceV1Summary::from_json(&null_period).expect("null period parses");
    assert_eq!(summary.parity().period, None);

    let missing = FINANCE_V1_SUMMARY.replace("  \"period\": \"2026-08\",\n", "");
    assert_ne!(missing, FINANCE_V1_SUMMARY);
    let error = FinanceV1Summary::from_json(&missing).expect_err("missing period is drift");
    assert!(matches!(error, ShadowError::Json(_)));
}
