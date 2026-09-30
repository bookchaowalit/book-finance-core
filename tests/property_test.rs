//! Deterministic property checks over many generated inputs.
//!
//! A fixed-seed generator keeps these reproducible and dependency-free; a
//! failure message prints the exact input so it can be turned into a unit test.

use book_finance_core::{
    AccountId, Currency, LedgerBook, LedgerEntry, Money, Posting, TransactionId,
};

const CURRENCIES: [Currency; 7] = [
    Currency::Thb,
    Currency::Usd,
    Currency::Eur,
    Currency::Gbp,
    Currency::Jpy,
    Currency::Btc,
    Currency::Eth,
];

/// SplitMix64: tiny, well-distributed, and stable across platforms.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn minor_units(&mut self) -> i128 {
        // Mix small values (where formatting edge cases live) with huge ones.
        let magnitude = match self.below(4) {
            0 => i128::from(self.below(1_000)),
            1 => i128::from(self.next()),
            2 => (i128::from(self.next()) << 48) | i128::from(self.next()),
            _ => i128::MAX - i128::from(self.below(1_000)),
        };
        if self.below(2) == 0 {
            -magnitude
        } else {
            magnitude
        }
    }
}

#[test]
fn decimal_strings_round_trip_through_parse() {
    let mut rng = Rng(0x5EED_0001);
    for _ in 0..20_000 {
        let currency = CURRENCIES[rng.below(CURRENCIES.len() as u64) as usize];
        let money = Money::new(currency, rng.minor_units());
        let text = money.to_decimal_string();
        assert_eq!(
            Money::parse(currency, &text),
            Ok(money),
            "{currency} {text}"
        );
    }
}

#[test]
fn allocation_conserves_the_amount_and_stays_within_one_minor_unit() {
    let mut rng = Rng(0x5EED_0002);
    for _ in 0..20_000 {
        let currency = CURRENCIES[rng.below(CURRENCIES.len() as u64) as usize];
        let money = Money::new(currency, rng.minor_units() >> rng.below(100));
        let count = 1 + rng.below(12) as usize;
        let weights: Vec<u64> = (0..count)
            .map(|_| match rng.below(4) {
                0 => 0,
                1 => rng.next(),
                _ => rng.below(100),
            })
            .collect();
        let total: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
        let Ok(parts) = money.allocate(&weights) else {
            assert!(
                total == 0 || total > u128::from(u64::MAX),
                "only an out-of-range weight total may fail: {money:?} {weights:?}"
            );
            continue;
        };
        assert_eq!(parts.len(), weights.len());
        let mut sum = Money::new(currency, 0);
        for (part, weight) in parts.iter().zip(&weights) {
            assert_eq!(part.currency(), currency);
            if *weight == 0 {
                assert!(part.is_zero(), "{money:?} {weights:?}");
            }
            // Each part is within one minor unit of its exact proportional share.
            let exact_floor = money.minor_units().unsigned_abs() / total * u128::from(*weight)
                + (money.minor_units().unsigned_abs() % total) * u128::from(*weight) / total;
            let got = part.minor_units().unsigned_abs();
            assert!(
                got == exact_floor || got == exact_floor + 1,
                "{money:?} {weights:?}"
            );
            sum = sum.checked_add(*part).expect("sum fits");
        }
        assert_eq!(sum, money, "{weights:?}");
    }
}

#[test]
fn balanced_entries_keep_the_book_summing_to_zero() {
    let mut rng = Rng(0x5EED_0003);
    let accounts: Vec<AccountId> = [
        "asset:cash",
        "asset:bank",
        "income:sales",
        "expense:ops",
        "liability:card",
    ]
    .into_iter()
    .map(|name| AccountId::new(name).expect("valid account"))
    .collect();
    let mut book = LedgerBook::default();
    for index in 0..2_000 {
        let amount = Money::new(Currency::Thb, 1 + i128::from(rng.below(10_000_000)));
        let debit = &accounts[rng.below(accounts.len() as u64) as usize];
        let credit = &accounts[rng.below(accounts.len() as u64) as usize];
        // Split the credit side so entries have more than two postings.
        let legs = amount
            .allocate(&[1 + rng.below(5), 1 + rng.below(5)])
            .expect("positive weights");
        let mut postings = vec![Posting::debit(debit.clone(), amount).expect("positive")];
        for leg in legs.into_iter().filter(|leg| !leg.is_zero()) {
            postings.push(Posting::credit(credit.clone(), leg).expect("positive"));
        }
        let entry = LedgerEntry::try_new(
            TransactionId::new(format!("tx-{index}")).expect("valid id"),
            postings,
        )
        .expect("balanced by construction");
        book.post(entry).expect("unique id");
    }
    let total = accounts
        .iter()
        .try_fold(Money::new(Currency::Thb, 0), |sum, account| {
            sum.checked_add(
                book.account_balance(account, Currency::Thb)
                    .expect("THB only"),
            )
        });
    assert_eq!(total, Ok(Money::new(Currency::Thb, 0)));
}
