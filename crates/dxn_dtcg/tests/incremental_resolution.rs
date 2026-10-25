use dxn_dtcg::{DtcgRuntime, Resolution};
use serde_json::{Map, Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test(unsupported = test)]
fn affected_resolution_visits_only_changed_tokens_and_their_dependency_closure() {
    let initial = resolver_document(0);
    let mut runtime = DtcgRuntime::from_json("large.resolver.json", initial).unwrap();
    let previous = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(previous.iter().count(), 10_000);

    let Resolution::Ready(affected) = runtime
        .replace_json_and_resolve_affected(
            "large.resolver.json",
            resolver_document(42),
            &[],
            &["token00000"],
        )
        .unwrap()
    else {
        panic!("the complete in-memory document must resolve");
    };

    assert_eq!(
        affected
            .iter()
            .map(|token| token.path.as_ref())
            .collect::<Vec<_>>(),
        ["token00000", "token00001", "token00002"]
    );
    assert_eq!(affected.nodes_visited(), 3);
    assert_eq!(previous.get("token09999").unwrap().value, json!(9_999));

    let current = runtime.resolve(&[]).unwrap().ready().unwrap();
    for token in affected.iter() {
        assert_eq!(current.get(&token.path).unwrap().value, token.value);
    }
    assert_eq!(current.get("token00000").unwrap().value, json!(42));
    assert_eq!(current.get("token00001").unwrap().value, json!(42));
    assert_eq!(current.get("token00002").unwrap().value, json!(42));
}

#[wasm_bindgen_test(unsupported = test)]
fn failed_affected_resolution_restores_the_previous_document() {
    let initial = resolver_document(7);
    let mut runtime = DtcgRuntime::from_json("large.resolver.json", initial).unwrap();
    let fingerprint = runtime
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
        .fingerprint()
        .to_owned();
    let mut invalid: Value = serde_json::from_str(&resolver_document(9)).unwrap();
    invalid["sets"]["large"]["sources"][0]["token00000"]["$value"] = json!("invalid");

    assert!(
        runtime
            .replace_json_and_resolve_affected(
                "large.resolver.json",
                serde_json::to_string(&invalid).unwrap(),
                &[],
                &["token00000"],
            )
            .is_err()
    );
    assert_eq!(
        runtime.resolve(&[]).unwrap().ready().unwrap().fingerprint(),
        fingerprint
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn group_extensions_fall_back_to_the_complete_projection_without_stale_tokens() {
    let initial = extended_resolver_document(1);
    let mut runtime = DtcgRuntime::from_json("extended.resolver.json", initial).unwrap();

    let affected = runtime
        .replace_json_and_resolve_affected(
            "extended.resolver.json",
            extended_resolver_document(9),
            &[],
            &["base.value"],
        )
        .unwrap()
        .ready()
        .unwrap();

    assert_eq!(affected.nodes_visited(), 2);
    assert_eq!(
        affected
            .iter()
            .map(|token| (token.path.as_ref(), token.value.clone()))
            .collect::<Vec<_>>(),
        [("base.value", json!(9)), ("derived.value", json!(9))]
    );
}

fn resolver_document(source_value: i64) -> String {
    let mut source = Map::with_capacity(10_000);
    for index in 0..10_000 {
        let value = match index {
            0 => json!(source_value),
            1 => json!("{token00000}"),
            2 => json!({ "$ref": "#/token00001/$value" }),
            _ => json!(index),
        };
        source.insert(
            format!("token{index:05}"),
            json!({ "$type": "number", "$value": value }),
        );
    }
    serde_json::to_string(&json!({
        "version": "2025.10",
        "sets": {
            "large": {
                "sources": [Value::Object(source)]
            }
        },
        "resolutionOrder": [{ "$ref": "#/sets/large" }]
    }))
    .unwrap()
}

fn extended_resolver_document(source_value: i64) -> String {
    serde_json::to_string(&json!({
        "version": "2025.10",
        "sets": {
            "extended": {
                "sources": [{
                    "base": {
                        "value": { "$type": "number", "$value": source_value }
                    },
                    "derived": { "$extends": "#/base" }
                }]
            }
        },
        "resolutionOrder": [{ "$ref": "#/sets/extended" }]
    }))
    .unwrap()
}
