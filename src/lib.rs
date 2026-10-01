#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

pub mod command;
pub mod shadow;

/// The read-contract seam that will later be consumed by a shadow reader.
pub const FINANCE_V1_SCHEMA_VERSION: &str = "finance.v1";
pub const FINANCE_V1_READ_PATH: &str = "/api/finance/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Currency {
    Thb,
    Usd,
    Eur,
    Gbp,
    Jpy,
    Btc,
    Eth,
}

impl Currency {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Thb => "THB",
            Self::Usd => "USD",
            Self::Eur => "EUR",
            Self::Gbp => "GBP",
            Self::Jpy => "JPY",
            Self::Btc => "BTC",
            Self::Eth => "ETH",
        }
    }

    pub const fn scale(self) -> u32 {
        match self {
            Self::Jpy => 0,
            Self::Btc => 8,
            Self::Eth => 18,
            Self::Thb | Self::Usd | Self::Eur | Self::Gbp => 2,
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for Currency {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "THB" => Ok(Self::Thb),
            "USD" => Ok(Self::Usd),
            "EUR" => Ok(Self::Eur),
            "GBP" => Ok(Self::Gbp),
            "JPY" => Ok(Self::Jpy),
            "BTC" => Ok(Self::Btc),
            "ETH" => Ok(Self::Eth),
            _ => Err(DomainError::InvalidCurrency),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyIdentifier {
        kind: &'static str,
    },
    InvalidAmount,
    InvalidCurrency,
    PrecisionExceeded {
        currency: Currency,
        scale: u32,
    },
    CurrencyMismatch {
        expected: Currency,
        actual: Currency,
    },
    NonPositivePosting,
    EmptyEntry,
    MixedCurrencies,
    UnbalancedEntry {
        currency: Currency,
        debit_minor: i128,
        credit_minor: i128,
    },
    DuplicateTransaction(TransactionId),
    /// Allocation needs a weight total between 1 and `u64::MAX`.
    InvalidAllocation,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier { kind } => {
                write!(formatter, "{kind} identifier cannot be empty")
            }
            Self::InvalidAmount => {
                formatter.write_str("amount is invalid or outside the supported range")
            }
            Self::InvalidCurrency => formatter.write_str("currency is not supported"),
            Self::PrecisionExceeded { currency, scale } => {
                write!(
                    formatter,
                    "{currency} supports at most {scale} fractional digits"
                )
            }
            Self::CurrencyMismatch { expected, actual } => {
                write!(
                    formatter,
                    "currency mismatch: expected {expected}, got {actual}"
                )
            }
            Self::NonPositivePosting => formatter.write_str("ledger postings must be positive"),
            Self::EmptyEntry => {
                formatter.write_str("ledger entry must contain at least one posting")
            }
            Self::MixedCurrencies => formatter.write_str("ledger entry cannot mix currencies"),
            Self::UnbalancedEntry {
                currency,
                debit_minor,
                credit_minor,
            } => write!(
                formatter,
                "entry is unbalanced in {currency}: debit={debit_minor}, credit={credit_minor}"
            ),
            Self::InvalidAllocation => {
                formatter.write_str("allocation weights must sum to between 1 and u64::MAX")
            }
            Self::DuplicateTransaction(transaction_id) => {
                write!(
                    formatter,
                    "transaction already posted: {}",
                    transaction_id.as_str()
                )
            }
        }
    }
}

impl std::error::Error for DomainError {}

/// A fixed-point amount stored as integer minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    currency: Currency,
    minor_units: i128,
}

impl Money {
    pub const fn new(currency: Currency, minor_units: i128) -> Self {
        Self {
            currency,
            minor_units,
        }
    }

    pub fn parse(currency: Currency, raw: &str) -> Result<Self, DomainError> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(DomainError::InvalidAmount);
        }

        let (negative, unsigned) = match value.as_bytes().first() {
            Some(b'-') => (true, &value[1..]),
            Some(b'+') => (false, &value[1..]),
            _ => (false, value),
        };
        let mut pieces = unsigned.split('.');
        let whole = pieces.next().unwrap_or_default();
        let fraction = pieces.next();
        // A trailing dot ("1.") is rejected: it is not a canonical amount and
        // `to_decimal_string` never produces it.
        if fraction == Some("") {
            return Err(DomainError::InvalidAmount);
        }
        let fraction = fraction.unwrap_or_default();
        if pieces.next().is_some()
            || whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(DomainError::InvalidAmount);
        }

        let scale = currency.scale();
        if fraction.len() > scale as usize {
            return Err(DomainError::PrecisionExceeded { currency, scale });
        }

        let whole_units = whole
            .parse::<i128>()
            .map_err(|_| DomainError::InvalidAmount)?;
        let scale_factor = 10_i128.pow(scale);
        let mut fraction_units = 0_i128;
        for byte in fraction.bytes() {
            fraction_units = fraction_units
                .checked_mul(10)
                .and_then(|value| value.checked_add(i128::from(byte - b'0')))
                .ok_or(DomainError::InvalidAmount)?;
        }
        for _ in fraction.len() as u32..scale {
            fraction_units = fraction_units
                .checked_mul(10)
                .ok_or(DomainError::InvalidAmount)?;
        }

        let magnitude = whole_units
            .checked_mul(scale_factor)
            .and_then(|value| value.checked_add(fraction_units))
            .ok_or(DomainError::InvalidAmount)?;
        let minor_units = if negative {
            magnitude.checked_neg().ok_or(DomainError::InvalidAmount)?
        } else {
            magnitude
        };

        Ok(Self::new(currency, minor_units))
    }

    pub const fn currency(self) -> Currency {
        self.currency
    }

    pub const fn minor_units(self) -> i128 {
        self.minor_units
    }

    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.ensure_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(DomainError::InvalidAmount)?;
        Ok(Self::new(self.currency, minor_units))
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, DomainError> {
        self.ensure_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(DomainError::InvalidAmount)?;
        Ok(Self::new(self.currency, minor_units))
    }

    pub fn checked_neg(self) -> Result<Self, DomainError> {
        Ok(Self::new(
            self.currency,
            self.minor_units
                .checked_neg()
                .ok_or(DomainError::InvalidAmount)?,
        ))
    }

    pub const fn is_zero(self) -> bool {
        self.minor_units == 0
    }

    /// Format the amount as a plain decimal string with exactly the
    /// currency's scale, e.g. `-12.34` for THB or `1500` for JPY. The output
    /// round-trips through [`Money::parse`] without loss.
    pub fn to_decimal_string(self) -> String {
        let scale = self.currency.scale() as usize;
        let sign = if self.minor_units < 0 { "-" } else { "" };
        let digits = self.minor_units.unsigned_abs().to_string();
        if scale == 0 {
            return format!("{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (whole, fraction) = padded.split_at(padded.len() - scale);
        format!("{sign}{whole}.{fraction}")
    }

    /// Split the amount into parts proportional to `weights` without floats.
    ///
    /// Uses the largest-remainder method on integer minor units: each part
    /// gets `floor(amount * weight / total)`, then the leftover minor units go
    /// one at a time to the parts with the largest remainders (ties go to the
    /// earlier part). The parts always sum exactly to `self`, have the same
    /// currency, and a zero weight always receives zero. Negative amounts are
    /// split by magnitude and negated, so `-x` splits as the negation of `x`.
    /// The weights must sum to at least 1 and at most `u64::MAX`, which keeps
    /// every intermediate product inside `u128`.
    pub fn allocate(self, weights: &[u64]) -> Result<Vec<Self>, DomainError> {
        let total_weight = weights
            .iter()
            .try_fold(0_u128, |sum, weight| sum.checked_add(u128::from(*weight)))
            .ok_or(DomainError::InvalidAllocation)?;
        if total_weight == 0 || total_weight > u128::from(u64::MAX) {
            return Err(DomainError::InvalidAllocation);
        }
        let magnitude = self.minor_units.unsigned_abs();
        let mut shares = Vec::with_capacity(weights.len());
        let mut remainders = Vec::with_capacity(weights.len());
        let mut allocated = 0_u128;
        for (index, weight) in weights.iter().enumerate() {
            let weight = u128::from(*weight);
            // magnitude * weight can exceed u128, so divide in two steps:
            // (q * total + r) * w / total = q * w + r * w / total, where
            // r < total <= u64::MAX and w <= u64::MAX, so r * w fits in u128.
            let quotient = magnitude / total_weight;
            let rest = magnitude % total_weight;
            let rest_product = rest.checked_mul(weight).ok_or(DomainError::InvalidAmount)?;
            let share = quotient
                .checked_mul(weight)
                .and_then(|value| value.checked_add(rest_product / total_weight))
                .ok_or(DomainError::InvalidAmount)?;
            shares.push(share);
            remainders.push((rest_product % total_weight, index));
            allocated += share;
        }
        let mut leftover = magnitude - allocated;
        remainders.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        for (remainder, index) in remainders {
            if leftover == 0 || remainder == 0 {
                break;
            }
            shares[index] += 1;
            leftover -= 1;
        }
        debug_assert_eq!(leftover, 0);
        shares
            .into_iter()
            .map(|share| {
                let share = i128::try_from(share).map_err(|_| DomainError::InvalidAmount)?;
                let signed = if self.minor_units < 0 { -share } else { share };
                Ok(Self::new(self.currency, signed))
            })
            .collect()
    }

    fn ensure_currency(self, other: Self) -> Result<(), DomainError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(DomainError::CurrencyMismatch {
                expected: self.currency,
                actual: other.currency,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(DomainError::EmptyIdentifier { kind: "account" });
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransactionId(String);

impl TransactionId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(DomainError::EmptyIdentifier {
                kind: "transaction",
            });
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Debit,
    Credit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    account_id: AccountId,
    amount: Money,
    direction: Direction,
}

impl Posting {
    pub fn debit(account_id: AccountId, amount: Money) -> Result<Self, DomainError> {
        Self::new(account_id, amount, Direction::Debit)
    }

    pub fn credit(account_id: AccountId, amount: Money) -> Result<Self, DomainError> {
        Self::new(account_id, amount, Direction::Credit)
    }

    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    pub const fn amount(&self) -> Money {
        self.amount
    }

    pub const fn direction(&self) -> Direction {
        self.direction
    }

    fn new(
        account_id: AccountId,
        amount: Money,
        direction: Direction,
    ) -> Result<Self, DomainError> {
        if amount.minor_units() <= 0 {
            return Err(DomainError::NonPositivePosting);
        }
        Ok(Self {
            account_id,
            amount,
            direction,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    transaction_id: TransactionId,
    postings: Vec<Posting>,
}

impl LedgerEntry {
    pub fn try_new(
        transaction_id: TransactionId,
        postings: impl IntoIterator<Item = Posting>,
    ) -> Result<Self, DomainError> {
        let postings: Vec<Posting> = postings.into_iter().collect();
        let first_posting = postings.first().ok_or(DomainError::EmptyEntry)?;
        let currency = first_posting.amount().currency();
        let mut debit_minor = 0_i128;
        let mut credit_minor = 0_i128;

        for posting in &postings {
            if posting.amount().currency() != currency {
                return Err(DomainError::MixedCurrencies);
            }
            match posting.direction() {
                Direction::Debit => {
                    debit_minor = debit_minor
                        .checked_add(posting.amount().minor_units())
                        .ok_or(DomainError::InvalidAmount)?;
                }
                Direction::Credit => {
                    credit_minor = credit_minor
                        .checked_add(posting.amount().minor_units())
                        .ok_or(DomainError::InvalidAmount)?;
                }
            }
        }

        if debit_minor != credit_minor {
            return Err(DomainError::UnbalancedEntry {
                currency,
                debit_minor,
                credit_minor,
            });
        }

        Ok(Self {
            transaction_id,
            postings,
        })
    }

    pub fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }
}

#[derive(Debug, Default, Clone)]
pub struct LedgerBook {
    entries: Vec<LedgerEntry>,
    transaction_ids: HashSet<TransactionId>,
}

impl LedgerBook {
    pub fn post(&mut self, entry: LedgerEntry) -> Result<(), DomainError> {
        if !self.transaction_ids.insert(entry.transaction_id().clone()) {
            return Err(DomainError::DuplicateTransaction(
                entry.transaction_id().clone(),
            ));
        }
        self.entries.push(entry);
        Ok(())
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn account_balance(
        &self,
        account_id: &AccountId,
        currency: Currency,
    ) -> Result<Money, DomainError> {
        let mut balance = Money::new(currency, 0);
        for entry in &self.entries {
            for posting in entry.postings() {
                if posting.account_id() != account_id {
                    continue;
                }
                if posting.amount().currency() != currency {
                    return Err(DomainError::CurrencyMismatch {
                        expected: currency,
                        actual: posting.amount().currency(),
                    });
                }
                let signed_amount = match posting.direction() {
                    Direction::Debit => posting.amount(),
                    Direction::Credit => posting.amount().checked_neg()?,
                };
                balance = balance.checked_add(signed_amount)?;
            }
        }
        Ok(balance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(value: &str) -> AccountId {
        AccountId::new(value).expect("valid account id")
    }

    fn transaction(value: &str) -> TransactionId {
        TransactionId::new(value).expect("valid transaction id")
    }

    fn thb(value: &str) -> Money {
        Money::parse(Currency::Thb, value).expect("valid THB amount")
    }

    fn balanced_entry(id: &str) -> LedgerEntry {
        LedgerEntry::try_new(
            transaction(id),
            [
                Posting::debit(account("asset:cash"), thb("100.00")).expect("valid debit"),
                Posting::credit(account("income:revenue"), thb("100.00")).expect("valid credit"),
            ],
        )
        .expect("balanced entry")
    }

    #[test]
    fn parses_fixed_point_money_without_float_rounding() {
        let amount = Money::parse(Currency::Thb, "12.34").expect("valid amount");

        assert_eq!(amount.currency(), Currency::Thb);
        assert_eq!(amount.minor_units(), 1_234);
    }

    #[test]
    fn rejects_amounts_with_excess_precision() {
        let error = Money::parse(Currency::Thb, "12.345").expect_err("precision must fail");

        assert_eq!(
            error,
            DomainError::PrecisionExceeded {
                currency: Currency::Thb,
                scale: 2,
            }
        );
    }

    #[test]
    fn prevents_cross_currency_arithmetic() {
        let error = thb("10.00")
            .checked_add(Money::parse(Currency::Usd, "10.00").expect("valid USD"))
            .expect_err("currencies must not be mixed");

        assert_eq!(
            error,
            DomainError::CurrencyMismatch {
                expected: Currency::Thb,
                actual: Currency::Usd,
            }
        );
    }

    #[test]
    fn rejects_unbalanced_entries() {
        let error = LedgerEntry::try_new(
            transaction("tx-unbalanced"),
            [
                Posting::debit(account("asset:cash"), thb("100.00")).expect("valid debit"),
                Posting::credit(account("income:revenue"), thb("90.00")).expect("valid credit"),
            ],
        )
        .expect_err("entry must balance");

        assert_eq!(
            error,
            DomainError::UnbalancedEntry {
                currency: Currency::Thb,
                debit_minor: 10_000,
                credit_minor: 9_000,
            }
        );
    }

    #[test]
    fn enforces_idempotent_transaction_posting() {
        let entry = balanced_entry("tx-001");
        let mut book = LedgerBook::default();

        book.post(entry.clone()).expect("first post succeeds");
        let error = book.post(entry).expect_err("duplicate must fail");

        assert_eq!(
            error,
            DomainError::DuplicateTransaction(transaction("tx-001"))
        );
        assert_eq!(book.entries().len(), 1);
    }

    #[test]
    fn projects_signed_account_balances() {
        let mut book = LedgerBook::default();
        book.post(balanced_entry("tx-001")).expect("post succeeds");

        assert_eq!(
            book.account_balance(&account("asset:cash"), Currency::Thb)
                .expect("cash balance"),
            thb("100.00")
        );
        assert_eq!(
            book.account_balance(&account("income:revenue"), Currency::Thb)
                .expect("revenue balance"),
            thb("-100.00")
        );
    }

    #[test]
    fn parses_signs_and_pads_missing_fraction_digits() {
        assert_eq!(thb("-12.3").minor_units(), -1_230);
        assert_eq!(thb("+7").minor_units(), 700);
        assert_eq!(thb(" 0.05 ").minor_units(), 5);
        assert!(thb("-0.00").is_zero());
    }

    #[test]
    fn applies_each_currency_scale_exactly() {
        let jpy = Money::parse(Currency::Jpy, "1500").expect("valid JPY");
        assert_eq!(jpy.minor_units(), 1_500);
        assert_eq!(
            Money::parse(Currency::Jpy, "1.5"),
            Err(DomainError::PrecisionExceeded {
                currency: Currency::Jpy,
                scale: 0,
            })
        );

        let btc = Money::parse(Currency::Btc, "0.00000001").expect("one satoshi");
        assert_eq!(btc.minor_units(), 1);

        let eth = Money::parse(Currency::Eth, "1.000000000000000001").expect("valid wei");
        assert_eq!(eth.minor_units(), 1_000_000_000_000_000_001);
    }

    #[test]
    fn rejects_malformed_amounts() {
        for raw in [
            "", " ", "-", "+", ".5", "1.2.3", "1,000.00", "1e3", "0x10", "12a", "--1", "+-1",
            "NaN", "inf", "１２", "1.", "-1.", "+0.", "1. ",
        ] {
            assert_eq!(
                Money::parse(Currency::Thb, raw),
                Err(DomainError::InvalidAmount),
                "{raw:?} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_amounts_that_overflow_minor_units() {
        let too_large = format!("{}", i128::MAX);
        assert_eq!(
            Money::parse(Currency::Thb, &too_large),
            Err(DomainError::InvalidAmount)
        );
        assert_eq!(
            Money::new(Currency::Thb, i128::MAX).checked_add(thb("0.01")),
            Err(DomainError::InvalidAmount)
        );
        assert_eq!(
            Money::new(Currency::Thb, i128::MIN).checked_neg(),
            Err(DomainError::InvalidAmount)
        );
    }

    #[test]
    fn formats_decimal_strings_that_round_trip() {
        let cases = [
            (Currency::Thb, 1_234, "12.34"),
            (Currency::Thb, -5, "-0.05"),
            (Currency::Thb, 0, "0.00"),
            (Currency::Jpy, -1_500, "-1500"),
            (Currency::Btc, 1, "0.00000001"),
            (Currency::Eth, 10_i128.pow(18), "1.000000000000000000"),
            (
                Currency::Usd,
                i128::MIN,
                "-1701411834604692317316873037158841057.28",
            ),
        ];
        for (currency, minor_units, expected) in cases {
            let money = Money::new(currency, minor_units);
            assert_eq!(money.to_decimal_string(), expected);
            if minor_units != i128::MIN {
                assert_eq!(Money::parse(currency, expected), Ok(money));
            }
        }
    }

    #[test]
    fn postings_must_be_strictly_positive() {
        assert_eq!(
            Posting::debit(account("asset:cash"), thb("0.00")),
            Err(DomainError::NonPositivePosting)
        );
        assert_eq!(
            Posting::credit(account("asset:cash"), thb("-1.00")),
            Err(DomainError::NonPositivePosting)
        );
    }

    #[test]
    fn rejects_empty_and_mixed_currency_entries() {
        assert_eq!(
            LedgerEntry::try_new(transaction("tx-empty"), []),
            Err(DomainError::EmptyEntry)
        );
        let usd = Money::parse(Currency::Usd, "100.00").expect("valid USD");
        assert_eq!(
            LedgerEntry::try_new(
                transaction("tx-mixed"),
                [
                    Posting::debit(account("asset:cash"), thb("100.00")).expect("valid debit"),
                    Posting::credit(account("income:revenue"), usd).expect("valid credit"),
                ],
            ),
            Err(DomainError::MixedCurrencies)
        );
    }

    #[test]
    fn rejects_blank_identifiers() {
        assert_eq!(
            AccountId::new("   "),
            Err(DomainError::EmptyIdentifier { kind: "account" })
        );
        assert_eq!(
            TransactionId::new(""),
            Err(DomainError::EmptyIdentifier {
                kind: "transaction"
            })
        );
    }

    #[test]
    fn balance_projection_fails_closed_on_currency_mismatch() {
        let mut book = LedgerBook::default();
        book.post(balanced_entry("tx-001")).expect("post succeeds");

        assert_eq!(
            book.account_balance(&account("asset:cash"), Currency::Usd),
            Err(DomainError::CurrencyMismatch {
                expected: Currency::Usd,
                actual: Currency::Thb,
            })
        );
        assert_eq!(
            book.account_balance(&account("asset:unused"), Currency::Thb),
            Ok(Money::new(Currency::Thb, 0))
        );
    }

    #[test]
    fn allocates_with_largest_remainder_and_exact_sum() {
        let parts = thb("100.00").allocate(&[1, 1, 1]).expect("valid split");
        assert_eq!(parts, vec![thb("33.34"), thb("33.33"), thb("33.33")]);

        let parts = thb("0.05").allocate(&[3, 0, 7]).expect("valid split");
        assert_eq!(parts, vec![thb("0.02"), thb("0.00"), thb("0.03")]);

        let parts = thb("-100.00").allocate(&[1, 1, 1]).expect("valid split");
        assert_eq!(parts, vec![thb("-33.34"), thb("-33.33"), thb("-33.33")]);

        let jpy = Money::new(Currency::Jpy, 10);
        assert_eq!(
            jpy.allocate(&[1, 2]).expect("valid split"),
            vec![Money::new(Currency::Jpy, 3), Money::new(Currency::Jpy, 7)]
        );
    }

    #[test]
    fn allocation_handles_extreme_amounts_and_weights() {
        let big = Money::new(Currency::Eth, i128::MAX);
        let parts = big
            .allocate(&[u64::MAX / 2, u64::MAX / 2, 1])
            .expect("valid split");
        let sum = parts
            .iter()
            .try_fold(Money::new(Currency::Eth, 0), |acc, part| {
                acc.checked_add(*part)
            })
            .expect("sum fits");
        assert_eq!(sum, big);

        let min = Money::new(Currency::Thb, i128::MIN);
        assert_eq!(min.allocate(&[1]), Err(DomainError::InvalidAmount));
    }

    #[test]
    fn allocation_rejects_missing_or_zero_weights() {
        assert_eq!(
            thb("1.00").allocate(&[]),
            Err(DomainError::InvalidAllocation)
        );
        assert_eq!(
            thb("1.00").allocate(&[0, 0]),
            Err(DomainError::InvalidAllocation)
        );
        assert_eq!(
            thb("1.00").allocate(&[u64::MAX, 1]),
            Err(DomainError::InvalidAllocation)
        );
    }

    #[test]
    fn exposes_the_finance_v1_contract_seam() {
        assert_eq!(FINANCE_V1_SCHEMA_VERSION, "finance.v1");
        assert_eq!(FINANCE_V1_READ_PATH, "/api/finance/v1");
    }
}
