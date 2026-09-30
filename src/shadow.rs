use crate::{Currency, DomainError, FINANCE_V1_SCHEMA_VERSION, Money};
use serde::de::Error as DeserializeError;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, str::FromStr};

pub const FINANCE_V1_SERVICE: &str = "book-finance-api";
pub const FINANCE_V1_SOURCE_OF_TRUTH: &str = "sqlite.finance_control_plane";

#[derive(Debug)]
pub enum ShadowError {
    Json(serde_json::Error),
    Domain(DomainError),
    WrongSchema {
        expected: &'static str,
        actual: String,
    },
    WrongService {
        expected: &'static str,
        actual: String,
    },
    WrongSourceOfTruth {
        expected: &'static str,
        actual: String,
    },
    TransactionNetMismatch {
        currency: Currency,
        expected: Money,
        actual: Money,
    },
    RevenueTotalMismatch {
        currency: Currency,
        expected: Money,
        actual: Money,
    },
    DuplicateCurrencyGroup {
        section: &'static str,
        currency: Currency,
    },
    /// Two invoice rows share a status and currency, so a consumer summing
    /// or keying by (status, currency) would double count or drop one.
    DuplicateInvoiceGroup {
        status: String,
        currency: Currency,
    },
    /// An invoice status that is empty or has surrounding whitespace.
    InvalidInvoiceStatus,
}

impl fmt::Display for ShadowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "finance.v1 JSON is invalid: {error}"),
            Self::Domain(error) => write!(formatter, "finance.v1 domain value is invalid: {error}"),
            Self::WrongSchema { expected, actual } => {
                write!(formatter, "expected schema {expected}, got {actual}")
            }
            Self::WrongService { expected, actual } => {
                write!(formatter, "expected service {expected}, got {actual}")
            }
            Self::WrongSourceOfTruth { expected, actual } => {
                write!(
                    formatter,
                    "expected source_of_truth {expected}, got {actual}"
                )
            }
            Self::TransactionNetMismatch {
                currency,
                expected,
                actual,
            } => write!(
                formatter,
                "transaction net mismatch in {currency}: expected minor units {}, got {}",
                expected.minor_units(),
                actual.minor_units()
            ),
            Self::RevenueTotalMismatch {
                currency,
                expected,
                actual,
            } => write!(
                formatter,
                "revenue total mismatch in {currency}: expected minor units {}, got {}",
                expected.minor_units(),
                actual.minor_units()
            ),
            Self::DuplicateCurrencyGroup { section, currency } => {
                write!(formatter, "duplicate {currency} group in {section}")
            }
            Self::DuplicateInvoiceGroup { status, currency } => {
                write!(formatter, "duplicate {status}/{currency} group in invoices")
            }
            Self::InvalidInvoiceStatus => formatter
                .write_str("invoice status must be non-empty without surrounding whitespace"),
        }
    }
}

impl std::error::Error for ShadowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Domain(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ShadowError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<DomainError> for ShadowError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionAggregate {
    pub currency: Currency,
    pub count: u64,
    pub income: Money,
    pub expense: Money,
    pub net: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevenueAggregate {
    pub currency: Currency,
    pub count: u64,
    pub total: Money,
    pub received: Money,
    pub pending: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceAggregate {
    pub status: String,
    pub currency: Currency,
    pub count: u64,
    pub amount: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiabilityAggregate {
    pub currency: Currency,
    pub count: u64,
    pub outstanding: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceV1Summary {
    pub schema_version: String,
    pub service: String,
    pub period: Option<String>,
    pub source_of_truth: String,
    pub transactions: Vec<TransactionAggregate>,
    pub revenue: Vec<RevenueAggregate>,
    pub invoices: Vec<InvoiceAggregate>,
    pub accounts_active: u64,
    pub liabilities: Vec<LiabilityAggregate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShadowParity {
    pub schema_version: &'static str,
    pub service: &'static str,
    pub period: Option<String>,
    pub transaction_groups: usize,
    pub revenue_groups: usize,
    pub invoice_groups: usize,
    pub active_accounts: u64,
    pub liability_groups: usize,
}

impl FinanceV1Summary {
    /// Parse and validate a sanitized finance.v1 summary without opening any
    /// database, network connection, or credential provider.
    pub fn from_json(input: &str) -> Result<Self, ShadowError> {
        let raw: RawSummary = serde_json::from_str(input)?;
        if raw.schema_version != FINANCE_V1_SCHEMA_VERSION {
            return Err(ShadowError::WrongSchema {
                expected: FINANCE_V1_SCHEMA_VERSION,
                actual: raw.schema_version,
            });
        }
        if raw.service != FINANCE_V1_SERVICE {
            return Err(ShadowError::WrongService {
                expected: FINANCE_V1_SERVICE,
                actual: raw.service,
            });
        }
        if raw.source_of_truth != FINANCE_V1_SOURCE_OF_TRUTH {
            return Err(ShadowError::WrongSourceOfTruth {
                expected: FINANCE_V1_SOURCE_OF_TRUTH,
                actual: raw.source_of_truth,
            });
        }

        let transactions = raw
            .transactions
            .into_iter()
            .map(TransactionAggregate::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        ensure_unique_currency_groups(
            "transactions",
            transactions.iter().map(|item| item.currency),
        )?;

        let revenue = raw
            .revenue
            .into_iter()
            .map(RevenueAggregate::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        ensure_unique_currency_groups("revenue", revenue.iter().map(|item| item.currency))?;

        let invoices = raw
            .invoices
            .into_iter()
            .map(InvoiceAggregate::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let mut invoice_groups = HashSet::new();
        for invoice in &invoices {
            if !invoice_groups.insert((invoice.status.as_str(), invoice.currency)) {
                return Err(ShadowError::DuplicateInvoiceGroup {
                    status: invoice.status.clone(),
                    currency: invoice.currency,
                });
            }
        }

        let liabilities = raw
            .liabilities
            .into_iter()
            .map(LiabilityAggregate::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        ensure_unique_currency_groups("liabilities", liabilities.iter().map(|item| item.currency))?;

        Ok(Self {
            schema_version: FINANCE_V1_SCHEMA_VERSION.to_owned(),
            service: FINANCE_V1_SERVICE.to_owned(),
            period: raw.period,
            source_of_truth: FINANCE_V1_SOURCE_OF_TRUTH.to_owned(),
            transactions,
            revenue,
            invoices,
            accounts_active: raw.accounts.active,
            liabilities,
        })
    }

    pub fn parity(&self) -> ShadowParity {
        ShadowParity {
            schema_version: FINANCE_V1_SCHEMA_VERSION,
            service: FINANCE_V1_SERVICE,
            period: self.period.clone(),
            transaction_groups: self.transactions.len(),
            revenue_groups: self.revenue.len(),
            invoice_groups: self.invoices.len(),
            active_accounts: self.accounts_active,
            liability_groups: self.liabilities.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JsonAmount(String);

impl<'de> Deserialize<'de> for JsonAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Number(number) => Ok(Self(number.to_string())),
            serde_json::Value::String(value) if !value.trim().is_empty() => Ok(Self(value)),
            _ => Err(D::Error::custom(
                "finance.v1 amount must be a JSON number or decimal string",
            )),
        }
    }
}

impl JsonAmount {
    fn money(self, currency: Currency) -> Result<Money, ShadowError> {
        Money::parse(currency, &self.0).map_err(ShadowError::Domain)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSummary {
    schema_version: String,
    service: String,
    /// Required but nullable: a missing key is contract drift (for example a
    /// rename), not an unscoped period. `deserialize_with` turns off serde's
    /// implicit `None` for a missing `Option` field.
    #[serde(deserialize_with = "required_nullable_string")]
    period: Option<String>,
    source_of_truth: String,
    transactions: Vec<RawTransactionAggregate>,
    revenue: Vec<RawRevenueAggregate>,
    invoices: Vec<RawInvoiceAggregate>,
    accounts: RawAccounts,
    liabilities: Vec<RawLiabilityAggregate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTransactionAggregate {
    currency: String,
    count: u64,
    income: JsonAmount,
    expense: JsonAmount,
    net: JsonAmount,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRevenueAggregate {
    currency: String,
    count: u64,
    total: JsonAmount,
    received: JsonAmount,
    pending: JsonAmount,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInvoiceAggregate {
    status: String,
    currency: String,
    count: u64,
    amount: JsonAmount,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAccounts {
    active: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLiabilityAggregate {
    currency: String,
    count: u64,
    outstanding: JsonAmount,
}

impl TryFrom<RawTransactionAggregate> for TransactionAggregate {
    type Error = ShadowError;

    fn try_from(raw: RawTransactionAggregate) -> Result<Self, Self::Error> {
        let currency = parse_currency(&raw.currency)?;
        let income = raw.income.money(currency)?;
        let expense = raw.expense.money(currency)?;
        let net = raw.net.money(currency)?;
        let expected_net = income.checked_sub(expense)?;
        if expected_net != net {
            return Err(ShadowError::TransactionNetMismatch {
                currency,
                expected: expected_net,
                actual: net,
            });
        }
        Ok(Self {
            currency,
            count: raw.count,
            income,
            expense,
            net,
        })
    }
}

impl TryFrom<RawRevenueAggregate> for RevenueAggregate {
    type Error = ShadowError;

    fn try_from(raw: RawRevenueAggregate) -> Result<Self, Self::Error> {
        let currency = parse_currency(&raw.currency)?;
        let total = raw.total.money(currency)?;
        let received = raw.received.money(currency)?;
        let pending = raw.pending.money(currency)?;
        let expected_total = received.checked_add(pending)?;
        if expected_total != total {
            return Err(ShadowError::RevenueTotalMismatch {
                currency,
                expected: expected_total,
                actual: total,
            });
        }
        Ok(Self {
            currency,
            count: raw.count,
            total,
            received,
            pending,
        })
    }
}

impl TryFrom<RawInvoiceAggregate> for InvoiceAggregate {
    type Error = ShadowError;

    fn try_from(raw: RawInvoiceAggregate) -> Result<Self, Self::Error> {
        if raw.status.is_empty() || raw.status.trim() != raw.status {
            return Err(ShadowError::InvalidInvoiceStatus);
        }
        let currency = parse_currency(&raw.currency)?;
        Ok(Self {
            status: raw.status,
            currency,
            count: raw.count,
            amount: raw.amount.money(currency)?,
        })
    }
}

impl TryFrom<RawLiabilityAggregate> for LiabilityAggregate {
    type Error = ShadowError;

    fn try_from(raw: RawLiabilityAggregate) -> Result<Self, Self::Error> {
        let currency = parse_currency(&raw.currency)?;
        Ok(Self {
            currency,
            count: raw.count,
            outstanding: raw.outstanding.money(currency)?,
        })
    }
}

fn required_nullable_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

fn parse_currency(value: &str) -> Result<Currency, ShadowError> {
    Currency::from_str(value).map_err(ShadowError::Domain)
}

fn ensure_unique_currency_groups(
    section: &'static str,
    currencies: impl IntoIterator<Item = Currency>,
) -> Result<(), ShadowError> {
    let mut seen = HashSet::new();
    for currency in currencies {
        if !seen.insert(currency) {
            return Err(ShadowError::DuplicateCurrencyGroup { section, currency });
        }
    }
    Ok(())
}
