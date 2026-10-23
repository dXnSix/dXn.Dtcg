use dxn_dtcg::{DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

/// Resolves a resolver whose one set holds `sources` inline, in order.
fn resolve(sources: Vec<Value>) -> ResolvedTokens {
    let document = json!({"version": "2025.10", "sets": {"base": {"sources": sources}},
        "resolutionOrder": [{"$ref": "#/sets/base"}]});
    DtcgRuntime::from_json("root.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

fn number(value: i64) -> Value {
    json!({"$type": "number", "$value": value})
}

#[test]
fn group_and_root_extensions_merge_member_by_member() {
    let tokens = resolve(vec![
        json!({
            "$extensions": {"vendor": {"a": 1, "b": 1, "deep": {"x": 1}}},
            "g": {"$extensions": {"v": {"a": 1, "b": 1}}, "t": number(1)}
        }),
        json!({
            "$extensions": {"vendor": {"b": 2, "deep": {"y": 2}}},
            "g": {"$extensions": {"v": {"b": 2}, "w": true}}
        }),
    ]);
    assert_eq!(
        tokens.group("g").unwrap().extensions,
        json!({"v": {"a": 1, "b": 2}, "w": true})
    );
    assert_eq!(
        tokens.group("").unwrap().extensions,
        json!({"vendor": {"a": 1, "b": 2, "deep": {"x": 1, "y": 2}}})
    );
}

#[test]
fn a_later_non_object_replaces_the_earlier_value() {
    let tokens = resolve(vec![
        json!({"g": {
            "$extensions": {"vendor": {"a": 1}, "list": [1, 2], "flag": {"on": true}},
            "t": number(1)
        }}),
        json!({"g": {"$extensions": {"vendor": 5, "list": [3], "flag": null}}}),
        json!({"g": {"$extensions": {"vendor": {"c": 1}}}}),
    ]);
    assert_eq!(
        tokens.group("g").unwrap().extensions,
        json!({"vendor": {"c": 1}, "list": [3], "flag": null})
    );
}

#[test]
fn a_pointer_into_an_earlier_source_survives_a_later_source() {
    let tokens = resolve(vec![
        json!({
            "$extensions": {"org.example": {"unit": 8}},
            "g": {"$extensions": {"org.example": {"columns": 12}}}
        }),
        json!({
            "$extensions": {"org.example": {"gutter": 4}},
            "g": {"$extensions": {"org.example": {"rows": 3}}}
        }),
        json!({
            "unit": {"$type": "number", "$ref": "#/$extensions/org.example/unit"},
            "gutter": {"$type": "number", "$ref": "#/$extensions/org.example/gutter"},
            "g": {"columns": {"$type": "number", "$ref": "#/g/$extensions/org.example/columns"}}
        }),
    ]);
    assert_eq!(tokens.get("unit").unwrap().value, 8);
    assert_eq!(tokens.get("gutter").unwrap().value, 4);
    assert_eq!(tokens.get("g.columns").unwrap().value, 12);
}

#[test]
fn token_extensions_are_replaced_with_the_token() {
    let tokens = resolve(vec![
        json!({"g": {
            "kept": {"$type": "number", "$value": 1, "$extensions": {"v": {"a": 1}}},
            "dropped": {"$type": "number", "$value": 1, "$extensions": {"v": {"a": 1}}}
        }}),
        json!({"g": {
            "kept": {"$type": "number", "$value": 2, "$extensions": {"v": {"b": 2}}},
            "dropped": number(2)
        }}),
    ]);
    assert_eq!(
        tokens.get("g.kept").unwrap().extensions,
        json!({"v": {"b": 2}})
    );
    assert_eq!(tokens.get("g.dropped").unwrap().extensions, Value::Null);
}

#[test]
fn extends_merges_extensions_the_same_way() {
    let tokens = resolve(vec![json!({
        "base": {"$extensions": {"v": {"a": 1, "b": 1}, "keep": 1}, "t": number(1)},
        "derived": {"$extends": "{base}", "$extensions": {"v": {"b": 2}, "keep": [1]}}
    })]);
    assert_eq!(
        tokens.group("derived").unwrap().extensions,
        json!({"v": {"a": 1, "b": 2}, "keep": [1]})
    );
    assert_eq!(
        tokens.group("base").unwrap().extensions,
        json!({"v": {"a": 1, "b": 1}, "keep": 1})
    );
}
