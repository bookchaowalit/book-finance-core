const FINANCE_V1_FIXTURE: &str = include_str!("../fixtures/finance-v1-summary.json");

#[test]
fn fixture_keeps_the_sanitized_finance_v1_read_contract_shape() {
    for field in [
        "\"schema_version\": \"finance.v1\"",
        "\"service\": \"book-finance-api\"",
        "\"source_of_truth\": \"sqlite.finance_control_plane\"",
        "\"transactions\": [",
        "\"revenue\": [",
        "\"invoices\": [",
        "\"accounts\": {",
        "\"liabilities\": [",
    ] {
        assert!(
            FINANCE_V1_FIXTURE.contains(field),
            "missing contract field: {field}"
        );
    }
}
