use book_finance_core::command::{
    CommandEnvelope, CommandExecutor, FINANCE_CORE_COMMANDS_ENABLED_ENV, feature_flag_enabled,
};
use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    if !feature_flag_enabled(env::var(FINANCE_CORE_COMMANDS_ENABLED_ENV).ok().as_deref()) {
        eprintln!(
            "finance command pilot disabled; set {FINANCE_CORE_COMMANDS_ENABLED_ENV}=true for local-only execution"
        );
        return ExitCode::from(2);
    }

    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("finance command input failed: {error}");
        return ExitCode::FAILURE;
    }

    let envelope = match CommandEnvelope::from_json(&input) {
        Ok(envelope) => envelope,
        Err(error) => {
            eprintln!("finance command rejected: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut executor = CommandExecutor::default();
    match executor.execute(envelope) {
        Ok(preview) => match serde_json::to_string(&preview) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("finance command output failed: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("finance command rejected: {error}");
            ExitCode::FAILURE
        }
    }
}
