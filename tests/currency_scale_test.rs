//! Pins each supported currency's minor-unit scale. Fiat values follow
//! ISO 4217 (JPY 0; THB, USD, EUR, GBP 2). BTC (8, satoshi) and ETH (18, wei)
//! are not ISO 4217 and follow their protocol base units. A change here
//! rescales every stored minor-unit amount, so it must be deliberate.

use book_finance_core::{Currency, Money};
use std::str::FromStr;

const TABLE: [(Currency, &str, u32); 7] = [
    (Currency::Thb, "THB", 2),
    (Currency::Usd, "USD", 2),
    (Currency::Eur, "EUR", 2),
    (Currency::Gbp, "GBP", 2),
    (Currency::Jpy, "JPY", 0),
    (Currency::Btc, "BTC", 8),
    (Currency::Eth, "ETH", 18),
];

#[test]
fn every_currency_has_its_pinned_code_and_scale() {
    for (currency, code, scale) in TABLE {
        assert_eq!(currency.code(), code);
        assert_eq!(currency.scale(), scale, "{code}");
        assert_eq!(currency.to_string(), code);
        assert_eq!(Currency::from_str(code).unwrap(), currency);
        assert_eq!(
            Currency::from_str(&format!(" {} ", code.to_lowercase())).unwrap(),
            currency
        );
    }
}

#[test]
fn table_is_exhaustive() {
    // Adding a variant without a row here fails to compile.
    for (currency, _, _) in TABLE {
        match currency {
            Currency::Thb
            | Currency::Usd
            | Currency::Eur
            | Currency::Gbp
            | Currency::Jpy
            | Currency::Btc
            | Currency::Eth => {}
        }
    }
    assert_eq!(TABLE.len(), 7);
}

#[test]
fn one_major_unit_is_ten_to_the_scale_minor_units() {
    for (currency, code, scale) in TABLE {
        let one = Money::parse(currency, "1").expect("whole unit parses");
        assert_eq!(one.minor_units(), 10_i128.pow(scale), "{code}");
        let smallest = if scale == 0 {
            "1".to_owned()
        } else {
            format!("0.{}1", "0".repeat(scale as usize - 1))
        };
        assert_eq!(
            Money::parse(currency, &smallest).unwrap().minor_units(),
            1,
            "{code}"
        );
        let too_precise = format!("0.{}1", "0".repeat(scale as usize));
        assert!(
            Money::parse(currency, &too_precise).is_err(),
            "{code} accepts {too_precise}"
        );
    }
}
