use book_finance_core::shadow::FinanceV1Summary;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("finance.v1 shadow input failed: {error}");
        return ExitCode::FAILURE;
    }

    match FinanceV1Summary::from_json(&input) {
        Ok(summary) => {
            let parity = summary.parity();
            println!(
                "schema_version={} service={} period={} transaction_groups={} revenue_groups={} invoice_groups={} active_accounts={} liability_groups={}",
                parity.schema_version,
                parity.service,
                parity.period.as_deref().unwrap_or("none"),
                parity.transaction_groups,
                parity.revenue_groups,
                parity.invoice_groups,
                parity.active_accounts,
                parity.liability_groups,
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("finance.v1 shadow parity failed: {error}");
            ExitCode::FAILURE
        }
    }
}
