//! Local-only command pilot for the Finance Platform migration.
//!
//! This module deliberately stops before the API and persistence boundaries.
//! It proves the command envelope, feature gate, and in-process idempotency
//! behavior against the already validated `finance.v1` read model.

use crate::shadow::{FinanceV1Summary, ShadowError, ShadowParity};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

pub const FINANCE_COMMAND_SCHEMA_VERSION: &str = "finance.command.v1";
pub const RECONCILIATION_PREVIEW_COMMAND: &str = "finance.reconciliation.preview";
pub const FINANCE_CORE_COMMANDS_ENABLED_ENV: &str = "FINANCE_CORE_COMMANDS_ENABLED";
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 255;

/// A constrained idempotency key suitable for a future command boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, CommandError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_IDEMPOTENCY_KEY_LENGTH
            || !value.bytes().enumerate().all(|(index, byte)| {
                (index == 0 && byte.is_ascii_alphanumeric())
                    || (index > 0 && (byte.is_ascii_alphanumeric() || b"-._:".contains(&byte)))
            })
        {
            return Err(CommandError::InvalidIdempotencyKey);
        }
        Ok(Self(value))
    }
}

#[derive(Debug)]
pub enum CommandError {
    FeatureDisabled,
    InvalidEnvelope,
    UnsupportedCommand,
    InvalidIdempotencyKey,
    IdempotencyConflict,
    ShadowValidation(ShadowError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FeatureDisabled => formatter.write_str("finance command pilot is disabled"),
            Self::InvalidEnvelope => formatter.write_str("finance command envelope is invalid"),
            Self::UnsupportedCommand => formatter.write_str("finance command is not supported"),
            Self::InvalidIdempotencyKey => formatter.write_str("idempotency_key is invalid"),
            Self::IdempotencyConflict => {
                formatter.write_str("idempotency_key was reused with different input")
            }
            Self::ShadowValidation(_) => formatter.write_str("finance.v1 shadow validation failed"),
        }
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ShadowValidation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ShadowError> for CommandError {
    fn from(error: ShadowError) -> Self {
        Self::ShadowValidation(error)
    }
}

/// The only command currently piloted. It is intentionally a preview and
/// cannot post a ledger entry, alter SQLite, or move money.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationPreview {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub status: &'static str,
    pub period: Option<String>,
    pub write_effects: bool,
    pub ledger_mutation: bool,
    pub parity: ShadowParity,
}

#[derive(Debug, Clone)]
pub struct CommandEnvelope {
    idempotency_key: IdempotencyKey,
    summary: FinanceV1Summary,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandEnvelope {
    command: String,
    idempotency_key: String,
    summary: serde_json::Value,
}

impl CommandEnvelope {
    /// Parse one command request without opening a network, database, or
    /// credential provider.
    pub fn from_json(input: &str) -> Result<Self, CommandError> {
        let raw: RawCommandEnvelope =
            serde_json::from_str(input).map_err(|_| CommandError::InvalidEnvelope)?;
        if raw.command != RECONCILIATION_PREVIEW_COMMAND {
            return Err(CommandError::UnsupportedCommand);
        }

        let idempotency_key = IdempotencyKey::new(raw.idempotency_key)?;
        let summary_json =
            serde_json::to_string(&raw.summary).map_err(|_| CommandError::InvalidEnvelope)?;
        let summary = FinanceV1Summary::from_json(&summary_json)?;
        Ok(Self {
            idempotency_key,
            summary,
        })
    }
}

#[derive(Debug, Default)]
pub struct CommandExecutor {
    executed: HashMap<IdempotencyKey, (FinanceV1Summary, ReconciliationPreview)>,
}

impl CommandExecutor {
    /// Execute the local preview command with in-process idempotency.
    ///
    /// A future API adapter must replace this in-memory registry with a
    /// durable, unique idempotency record before any command can mutate data.
    pub fn execute(
        &mut self,
        envelope: CommandEnvelope,
    ) -> Result<ReconciliationPreview, CommandError> {
        if let Some((previous_summary, previous_preview)) =
            self.executed.get(&envelope.idempotency_key)
        {
            if previous_summary == &envelope.summary {
                return Ok(previous_preview.clone());
            }
            return Err(CommandError::IdempotencyConflict);
        }

        let preview = ReconciliationPreview {
            schema_version: FINANCE_COMMAND_SCHEMA_VERSION,
            command: RECONCILIATION_PREVIEW_COMMAND,
            status: "preview_only",
            period: envelope.summary.period.clone(),
            write_effects: false,
            ledger_mutation: false,
            parity: envelope.summary.parity(),
        };
        self.executed.insert(
            envelope.idempotency_key,
            (envelope.summary, preview.clone()),
        );
        Ok(preview)
    }
}

/// Feature flags are exact and fail closed. Any value other than lowercase
/// `true` keeps the pilot disabled.
pub fn feature_flag_enabled(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = include_str!("../fixtures/finance-v1-summary.json");

    fn envelope(key: &str, summary: &str) -> CommandEnvelope {
        let input = format!(
            r#"{{"command":"{RECONCILIATION_PREVIEW_COMMAND}","idempotency_key":"{key}","summary":{summary}}}"#
        );
        CommandEnvelope::from_json(&input).expect("valid command envelope")
    }

    #[test]
    fn feature_flag_fails_closed() {
        assert!(!feature_flag_enabled(None));
        assert!(!feature_flag_enabled(Some("TRUE")));
        assert!(!feature_flag_enabled(Some("1")));
        assert!(feature_flag_enabled(Some("true")));
    }

    #[test]
    fn preview_is_non_mutating_and_idempotent() {
        let mut executor = CommandExecutor::default();
        let first = executor
            .execute(envelope("preview-2026-08-001", SUMMARY))
            .expect("preview succeeds");
        let replay = executor
            .execute(envelope("preview-2026-08-001", SUMMARY))
            .expect("same key and input replay safely");

        assert_eq!(first, replay);
        assert_eq!(first.status, "preview_only");
        assert!(!first.write_effects);
        assert!(!first.ledger_mutation);
        assert_eq!(first.parity.transaction_groups, 1);
    }

    #[test]
    fn rejects_reusing_a_key_for_different_input() {
        let mut executor = CommandExecutor::default();
        executor
            .execute(envelope("preview-2026-08-002", SUMMARY))
            .expect("first preview succeeds");

        let changed_summary = SUMMARY.replace("\"period\": \"2026-08\"", "\"period\": \"2026-09\"");
        let error = executor
            .execute(envelope("preview-2026-08-002", &changed_summary))
            .expect_err("same key with different input must fail");
        assert!(matches!(error, CommandError::IdempotencyConflict));
    }

    #[test]
    fn rejects_unknown_envelope_fields() {
        let input = format!(
            r#"{{"command":"{RECONCILIATION_PREVIEW_COMMAND}","idempotency_key":"preview-001","summary":{SUMMARY},"unexpected":true}}"#
        );
        let error = CommandEnvelope::from_json(&input).expect_err("unknown field must fail");
        assert!(matches!(error, CommandError::InvalidEnvelope));
    }
}
