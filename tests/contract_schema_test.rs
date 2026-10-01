//! Keeps `fixtures/finance-v1.schema.json` (the documented read contract),
//! the sanitized fixture and the parser's accepted field sets in lockstep.

use book_finance_core::shadow::{FINANCE_V1_SERVICE, FINANCE_V1_SOURCE_OF_TRUTH, FinanceV1Summary};
use book_finance_core::{Currency, FINANCE_V1_SCHEMA_VERSION};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::str::FromStr;

const SCHEMA: &str = include_str!("../fixtures/finance-v1.schema.json");
const FIXTURE: &str = include_str!("../fixtures/finance-v1-summary.json");

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("valid JSON")
}

fn keys(object: &Map<String, Value>) -> BTreeSet<String> {
    object.keys().cloned().collect()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_owned())
        .collect()
}

/// (JSON pointer to the fixture object, schema node describing it).
fn objects(schema: &Value) -> Vec<(String, Value)> {
    let mut found = vec![(String::new(), schema.clone())];
    for (name, property) in schema["properties"].as_object().expect("properties") {
        match property["type"].as_str() {
            Some("array") => found.push((format!("/{name}/0"), property["items"].clone())),
            Some("object") => found.push((format!("/{name}"), property.clone())),
            _ => {}
        }
    }
    found
}

#[test]
fn schema_fixture_and_parser_accept_the_same_fields() {
    let schema = json(SCHEMA);
    let fixture = json(FIXTURE);
    FinanceV1Summary::from_json(FIXTURE).expect("fixture parses");

    for (pointer, node) in objects(&schema) {
        let properties = keys(node["properties"].as_object().expect("properties"));
        assert_eq!(
            strings(&node["required"]),
            properties,
            "{pointer}: every field is required"
        );
        assert_eq!(
            node["additionalProperties"],
            Value::Bool(false),
            "{pointer}"
        );
        let object = fixture
            .pointer(&pointer)
            .and_then(Value::as_object)
            .expect("fixture object");
        assert_eq!(
            keys(object),
            properties,
            "{pointer}: fixture and schema fields differ"
        );

        for field in &properties {
            let mut missing = fixture.clone();
            missing
                .pointer_mut(&pointer)
                .and_then(Value::as_object_mut)
                .unwrap()
                .remove(field);
            assert!(
                FinanceV1Summary::from_json(&missing.to_string()).is_err(),
                "{pointer}/{field} is required by the schema but the parser accepts it missing"
            );
        }
        let mut extra = fixture.clone();
        extra
            .pointer_mut(&pointer)
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("unexpected".into(), Value::Bool(true));
        assert!(
            FinanceV1Summary::from_json(&extra.to_string()).is_err(),
            "{pointer}: the parser accepts an unknown field"
        );
    }
}

#[test]
fn schema_constants_and_currencies_match_the_code() {
    let schema = json(SCHEMA);
    let properties = &schema["properties"];
    assert_eq!(
        properties["schema_version"]["const"],
        FINANCE_V1_SCHEMA_VERSION
    );
    assert_eq!(properties["service"]["const"], FINANCE_V1_SERVICE);
    assert_eq!(
        properties["source_of_truth"]["const"],
        FINANCE_V1_SOURCE_OF_TRUTH
    );

    let listed = strings(&schema["$defs"]["currency"]["enum"]);
    let supported: BTreeSet<String> = [
        Currency::Thb,
        Currency::Usd,
        Currency::Eur,
        Currency::Gbp,
        Currency::Jpy,
        Currency::Btc,
        Currency::Eth,
    ]
    .iter()
    .map(|currency| currency.code().to_owned())
    .collect();
    assert_eq!(listed, supported);
    for code in &listed {
        assert_eq!(
            Currency::from_str(code)
                .expect("listed currency parses")
                .code(),
            code
        );
    }
    assert!(
        Currency::from_str("CHF").is_err(),
        "an unlisted currency must stay unsupported"
    );
}
