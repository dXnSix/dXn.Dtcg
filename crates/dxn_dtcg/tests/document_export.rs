use std::{collections::BTreeMap, path::Path};

use dxn_dtcg::{DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

fn single(document: &Value) -> ResolvedTokens {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

fn layered(sources: &[Value]) -> ResolvedTokens {
    let references = (0..sources.len())
        .map(|index| json!({"$ref": format!("s{index}.json")}))
        .collect::<Vec<_>>();
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "all", "sources": references}]
        })
        .to_string(),
    )
    .unwrap();
    for (index, source) in sources.iter().enumerate() {
        runtime
            .provide_json(format!("s{index}.json"), source.to_string())
            .unwrap();
    }
    runtime.resolve(&[]).unwrap().ready().unwrap()
}

/// Exports `tokens`, resolves the export, and checks that every path, type,
/// value, description, deprecation and extension object survives. Paths are
/// compared as a set: a nested document groups tokens, so an interleaved
/// resolution order comes back grouped (see `to_document`).
fn assert_round_trip(tokens: &ResolvedTokens, label: &str) {
    let exported = tokens.to_document();
    let again = single(&exported);
    let describe = |tokens: &ResolvedTokens| {
        tokens
            .iter()
            .map(|token| {
                (
                    token.path.to_string(),
                    (
                        token.token_type.clone(),
                        token.value.clone(),
                        token.description.clone(),
                        token.deprecated.clone(),
                        token.extensions.clone(),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(describe(&again), describe(tokens), "{label}: {exported}");
}

/// The group-by-group order a nested document has: tokens in resolution
/// order, stably grouped by the order their top-level group first appears.
fn grouped_order(tokens: &ResolvedTokens) -> Vec<String> {
    let mut groups = Vec::<String>::new();
    for token in tokens.iter() {
        let group = token.path.split('.').next().unwrap().to_owned();
        if !groups.contains(&group) {
            groups.push(group);
        }
    }
    let mut paths = tokens
        .iter()
        .map(|token| token.path.to_string())
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        groups
            .iter()
            .position(|group| group == path.split('.').next().unwrap())
    });
    paths
}

#[test]
fn the_document_nests_tokens_with_explicit_types_and_their_metadata() {
    let tokens = layered(&[
        json!({
            "$extensions": {"vendor": {"a": 1}},
            "colors": {
                "$type": "color",
                "$description": "palette",
                "$deprecated": "use brand",
                "$extensions": {"group": true},
                "$root": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}},
                "red": {"$value": {"colorSpace": "srgb", "components": [1, 0, 0]}, "$description": "red", "$extensions": {"k": 1}},
                "kept": {"$value": "{colors.red}", "$deprecated": false}
            }
        }),
        json!({"$extensions": {"vendor": {"b": 2}}, "size": {"$type": "number", "$value": 4}}),
    ]);
    assert_eq!(
        tokens.to_document(),
        json!({
            "$extensions": {"vendor": {"a": 1, "b": 2}},
            "colors": {
                "$description": "palette",
                "$deprecated": "use brand",
                "$extensions": {"group": true},
                "$root": {"$type": "color", "$value": {"colorSpace": "srgb", "components": [0, 0, 0]}},
                "red": {
                    "$type": "color",
                    "$value": {"colorSpace": "srgb", "components": [1, 0, 0]},
                    "$description": "red",
                    "$extensions": {"k": 1}
                },
                "kept": {
                    "$type": "color",
                    "$value": {"colorSpace": "srgb", "components": [1, 0, 0]},
                    "$deprecated": false
                }
            },
            "size": {"$type": "number", "$value": 4}
        })
    );
    assert!(tokens.to_document().get("$schema").is_none());
}

/// Every resolving scenario of the `merged_resolution` and `token_metadata`
/// test groups, named after its test.
fn scenarios() -> Vec<(&'static str, ResolvedTokens)> {
    vec![
        (
            "merged_resolution::group_properties_merge_per_key_and_tokens_replace_whole",
            layered(&[
                json!({"g": {"$type": "number", "$description": "first", "$deprecated": "old", "$extensions": {"vendor": {"a": 1, "b": 1}}, "t": {"$value": 1, "$description": "kept?", "$extensions": {"x": 1}}}}),
                json!({"g": {"$description": "second", "$extensions": {"vendor": {"b": 2}}, "t": {"$value": 2}, "u": {"$value": 3}}}),
            ]),
        ),
        (
            "merged_resolution::later_tokens_inherit_group_types_declared_by_earlier_sources",
            layered(&[
                json!({"size": {"$type": "number", "sm": {"$value": 4}, "lg": {"$value": 16}}}),
                json!({"size": {"lg": {"$value": 12}}}),
            ]),
        ),
        (
            "merged_resolution::extends_reaches_groups_in_other_sources_and_groups_produced_by_extends",
            layered(&[
                json!({"base": {"$type": "number", "x": {"$value": 1}, "inner": {"$type": "number", "v": {"$value": 2}}}, "late": {"$extends": "{middle.inner}"}}),
                json!({"middle": {"$extends": "#/base", "y": {"$value": 3}}, "theme": {"$extends": "{base}", "x": {"$value": 10}}}),
            ]),
        ),
        (
            "merged_resolution::json_pointers_read_the_merged_structure",
            layered(&[
                json!({"$extensions": {"palette": {"red": 1, "blue": 2}}}),
                json!({
                    "$extensions": {"palette": {"blue": 20}},
                    "red": {"$type": "number", "$value": {"$ref": "#/$extensions/palette/red"}},
                    "blue": {"$type": "number", "$ref": "#/$extensions/palette/blue"},
                    "base": {"$type": "number", "$value": 7}
                }),
                json!({"copy": {"$type": "number", "$value": {"$ref": "#/base/$value"}}}),
            ]),
        ),
        (
            "merged_resolution::order_and_sources_keep_their_documented_meaning (overrides)",
            layered(&[
                json!({"a": {"$type": "number", "x": {"$value": 1}, "y": {"$value": 2}}, "b": {"$type": "number", "z": {"$value": 3}}}),
                json!({"a": {"x": {"$value": 10}}}),
            ]),
        ),
        (
            "merged_resolution::order_and_sources_keep_their_documented_meaning (extends)",
            layered(&[
                json!({"base": {"$type": "number", "p": {"$value": 1}, "q": {"$value": 2}}}),
                json!({"theme": {"own": {"$type": "number", "$value": 3}, "$extends": "{base}"}, "after": {"$type": "number", "$value": 4}}),
            ]),
        ),
        (
            "merged_resolution::previews_merge_the_previewed_source_last",
            DtcgRuntime::from_json(
                "tokens.json",
                json!({"size": {"$type": "number", "sm": {"$value": 4}, "lg": {"$value": 16}}})
                    .to_string(),
            )
            .unwrap()
            .preview_source(&json!({"size": {"lg": {"$value": 12}}}), &[])
            .unwrap()
            .ready()
            .unwrap(),
        ),
        (
            "merged_resolution::nested_groups_may_extend_the_same_target_but_real_cycles_fail",
            layered(&[json!({
                "base": {"$type": "number", "t": {"$value": 1}},
                "a": {"$extends": "{base}", "x": {"$extends": "{base}"}}
            })]),
        ),
        (
            "token_metadata::tokens_expose_their_own_description_deprecation_and_extensions",
            single(&json!({
                "g": {
                    "$type": "number",
                    "$description": "group text",
                    "$extensions": {"group": true},
                    "plain": {"$value": 1},
                    "described": {"$value": 2, "$description": "token text", "$extensions": {"k": 1}},
                    "flagged": {"$value": 3, "$deprecated": true},
                    "reasoned": {"$value": 4, "$deprecated": "Use {g.plain}"},
                    "kept": {"$value": 5, "$deprecated": false}
                }
            })),
        ),
        (
            "token_metadata::group_deprecation_is_inherited_until_a_nearer_value_overrides_it",
            single(&json!({
                "legacy": {
                    "$type": "number",
                    "$deprecated": "Use the new scale",
                    "a": {"$value": 1},
                    "b": {"$value": 2, "$deprecated": false},
                    "c": {"$value": 3, "$deprecated": true},
                    "inner": {"$deprecated": false, "d": {"$value": 4}, "deeper": {"$deprecated": true, "e": {"$value": 5}}}
                }
            })),
        ),
        (
            "token_metadata::deprecation_is_evaluated_on_the_merged_structure_and_through_extends",
            layered(&[
                json!({"old": {"$type": "number", "a": {"$value": 1}}}),
                json!({"old": {"$deprecated": true, "b": {"$value": 2}}}),
                json!({
                    "base": {"$type": "number", "$deprecated": "base is retired", "$description": "base", "x": {"$value": 1}},
                    "derived": {"$extends": "{base}", "y": {"$value": 2}},
                    "revived": {"$extends": "{base}", "$deprecated": false, "z": {"$value": 3}}
                }),
            ]),
        ),
        (
            "token_metadata::groups_with_resolved_tokens_are_listed_in_path_order",
            single(&json!({
                "$description": "root",
                "$extensions": {"vendor": {"x": 1}},
                "z": {"$type": "number", "t": {"$value": 1}},
                "a": {
                    "$type": "color",
                    "$description": "colors",
                    "nested": {"t": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}}},
                    "empty": {}
                },
                "unused": {"$description": "no tokens"}
            })),
        ),
        (
            "token_metadata::metadata_does_not_change_the_fingerprint (plain)",
            single(&json!({"g": {"$type": "number", "t": {"$value": 1}}})),
        ),
        (
            "token_metadata::metadata_does_not_change_the_fingerprint (annotated)",
            single(&json!({"g": {
                "$type": "number",
                "$description": "g",
                "$deprecated": true,
                "$extensions": {"k": 1},
                "t": {"$value": 1, "$description": "t", "$deprecated": "x", "$extensions": {"k": 2}}
            }})),
        ),
    ]
}

#[test]
fn merged_and_metadata_scenarios_round_trip() {
    for (label, tokens) in scenarios() {
        assert_round_trip(&tokens, label);
    }
}

#[test]
fn an_interleaved_resolution_order_comes_back_grouped() {
    let (_, tokens) = scenarios()
        .into_iter()
        .find(|(label, _)| label.ends_with("(overrides)"))
        .unwrap();
    let order = tokens
        .iter()
        .map(|token| token.path.to_string())
        .collect::<Vec<_>>();
    assert_eq!(order, ["a.y", "b.z", "a.x"]);
    let again = single(&tokens.to_document());
    let again_order = again
        .iter()
        .map(|token| token.path.to_string())
        .collect::<Vec<_>>();
    assert_eq!(again_order, ["a.y", "a.x", "b.z"]);
    assert_eq!(again_order, grouped_order(&tokens));
}

#[test]
fn every_positive_corpus_fixture_round_trips() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dtcg-test-suite/2025.10/format");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    let mut count = 0;
    for test in manifest["tests"].as_array().unwrap() {
        if test["type"] != "PositiveEvaluationTest" {
            continue;
        }
        let input = test["input"].as_str().unwrap();
        let document: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(input)).unwrap()).unwrap();
        assert_round_trip(&single(&document), input);
        count += 1;
    }
    assert_eq!(count, 93);
}
