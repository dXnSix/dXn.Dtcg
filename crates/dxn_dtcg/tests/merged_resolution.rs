use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

/// A runtime whose root resolver stacks `sources`, each provided as its own
/// document `s<index>.json`, in order.
fn layered(sources: &[Value]) -> DtcgRuntime {
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
    runtime
}

fn resolve(sources: &[Value]) -> Result<ResolvedTokens, DtcgError> {
    layered(sources)
        .resolve(&[])
        .map(|resolution| resolution.ready().expect("all documents are provided"))
}

fn paths(tokens: &ResolvedTokens) -> Vec<&str> {
    tokens.iter().map(|token| token.path.as_ref()).collect()
}

#[test]
fn group_properties_merge_per_key_and_tokens_replace_whole() {
    let tokens = resolve(&[
        json!({"g": {
            "$type": "number",
            "$description": "first",
            "$deprecated": "old",
            "$extensions": {"vendor": {"a": 1, "b": 1}},
            "t": {"$value": 1, "$description": "kept?", "$extensions": {"x": 1}}
        }}),
        json!({"g": {
            "$description": "second",
            "$extensions": {"vendor": {"b": 2}},
            "t": {"$value": 2},
            "u": {"$value": 3}
        }}),
    ])
    .unwrap();
    let group = tokens.group("g").unwrap();
    assert_eq!(group.token_type.as_deref(), Some("number"));
    assert_eq!(group.description.as_deref(), Some("second"));
    assert!(group.deprecated.as_ref().unwrap().is_deprecated());
    assert_eq!(group.extensions, json!({"vendor": {"a": 1, "b": 2}}));

    let replaced = tokens.get("g.t").unwrap();
    assert_eq!(replaced.value, json!(2));
    assert_eq!(replaced.description, None);
    assert_eq!(replaced.extensions, Value::Null);
    assert_eq!(replaced.source.as_str(), "s1.json");
    assert_eq!(
        tokens.get("g.u").unwrap().token_type.as_deref(),
        Some("number")
    );
}

#[test]
fn later_tokens_inherit_group_types_declared_by_earlier_sources() {
    // The overlay example of docs/06-resolvers-and-modifiers.md.
    let base = json!({"size": {"$type": "number", "sm": {"$value": 4}, "lg": {"$value": 16}}});
    let tokens = resolve(&[base, json!({"size": {"lg": {"$value": 12}}})]).unwrap();
    let large = tokens.get("size.lg").unwrap();
    assert_eq!(large.value, json!(12));
    assert_eq!(large.token_type.as_deref(), Some("number"));
    assert_eq!(large.source.as_str(), "s1.json");
    assert_eq!(tokens.get("size.sm").unwrap().value, json!(4));
}

#[test]
fn extends_reaches_groups_in_other_sources_and_groups_produced_by_extends() {
    let tokens = resolve(&[
        json!({
            "base": {"$type": "number", "x": {"$value": 1}, "inner": {"$type": "number", "v": {"$value": 2}}},
            "late": {"$extends": "{middle.inner}"}
        }),
        json!({
            "middle": {"$extends": "#/base", "y": {"$value": 3}},
            "theme": {"$extends": "{base}", "x": {"$value": 10}}
        }),
    ])
    .unwrap();
    assert_eq!(tokens.get("theme.x").unwrap().value, json!(10));
    assert_eq!(tokens.get("theme.inner.v").unwrap().value, json!(2));
    assert_eq!(
        tokens.get("middle.x").unwrap().token_type.as_deref(),
        Some("number")
    );
    // `middle.inner` exists only through `middle`'s `$extends`.
    assert_eq!(tokens.get("late.v").unwrap().value, json!(2));
    assert_eq!(
        tokens.get("late.v").unwrap().token_type.as_deref(),
        Some("number")
    );

    let cycle = resolve(&[
        json!({"a": {"$extends": "{b}"}}),
        json!({"b": {"$extends": "{a}"}}),
    ])
    .unwrap_err();
    assert!(
        matches!(&cycle, DtcgError::Token { message, .. } if message.starts_with("group extension cycle")),
        "{cycle:?}"
    );
}

#[test]
fn a_path_that_is_a_token_in_one_source_and_a_group_in_another_collides() {
    let token = json!({"p": {"$type": "number", "$value": 1}});
    let group = json!({"p": {"child": {"$type": "number", "$value": 2}}});
    for (sources, message) in [
        (
            [token.clone(), group.clone()],
            "path is a token in s0.json and a group in s1.json",
        ),
        (
            [group, token],
            "path is a token in s1.json and a group in s0.json",
        ),
    ] {
        let error = resolve(&sources).unwrap_err();
        assert!(
            matches!(&error, DtcgError::Token { path, message: reported, .. }
                if path == "p" && reported == message),
            "{error:?}"
        );
    }
}

#[test]
fn json_pointers_read_the_merged_structure() {
    let tokens = resolve(&[
        json!({"$extensions": {"palette": {"red": 1, "blue": 2}}}),
        json!({
            "$extensions": {"palette": {"blue": 20}},
            "red": {"$type": "number", "$value": {"$ref": "#/$extensions/palette/red"}},
            "blue": {"$type": "number", "$ref": "#/$extensions/palette/blue"},
            "base": {"$type": "number", "$value": 7}
        }),
        json!({"copy": {"$type": "number", "$value": {"$ref": "#/base/$value"}}}),
    ])
    .unwrap();
    assert_eq!(tokens.get("red").unwrap().value, json!(1));
    assert_eq!(tokens.get("blue").unwrap().value, json!(20));
    assert_eq!(tokens.get("copy").unwrap().value, json!(7));
    assert_eq!(
        tokens.group("").unwrap().extensions,
        json!({"palette": {"red": 1, "blue": 20}})
    );
}

#[test]
fn order_and_sources_keep_their_documented_meaning() {
    let tokens = resolve(&[
        json!({"a": {"$type": "number", "x": {"$value": 1}, "y": {"$value": 2}}, "b": {"$type": "number", "z": {"$value": 3}}}),
        json!({"a": {"x": {"$value": 10}}}),
    ])
    .unwrap();
    assert_eq!(paths(&tokens), ["a.y", "b.z", "a.x"]);
    assert_eq!(tokens.get("a.x").unwrap().source.as_str(), "s1.json");
    assert_eq!(tokens.get("a.y").unwrap().source.as_str(), "s0.json");

    // Tokens copied by `$extends` take the position of the extending group
    // and name the document of its `$extends`.
    let tokens = resolve(&[
        json!({"base": {"$type": "number", "p": {"$value": 1}, "q": {"$value": 2}}}),
        json!({"theme": {"own": {"$type": "number", "$value": 3}, "$extends": "{base}"}, "after": {"$type": "number", "$value": 4}}),
    ])
    .unwrap();
    assert_eq!(
        paths(&tokens),
        [
            "base.p",
            "base.q",
            "theme.p",
            "theme.q",
            "theme.own",
            "after"
        ]
    );
    assert_eq!(tokens.get("theme.p").unwrap().source.as_str(), "s1.json");
}

#[test]
fn previews_merge_the_previewed_source_last() {
    let runtime =
        layered(&[json!({"size": {"$type": "number", "sm": {"$value": 4}, "lg": {"$value": 16}}})]);
    let preview = runtime
        .preview_source(&json!({"size": {"lg": {"$value": 12}}}), &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(preview.get("size.lg").unwrap().value, json!(12));
    assert_eq!(
        preview.get("size.lg").unwrap().token_type.as_deref(),
        Some("number")
    );
    assert_eq!(paths(&preview), ["size.sm", "size.lg"]);

    let occurrences = runtime
        .inspect_source(&json!({"$ref": "s0.json"}), &[])
        .unwrap()
        .ready()
        .unwrap();
    let occurrence = runtime
        .preview_occurrence(&occurrences[0], &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(occurrence.get("size.lg").unwrap().value, json!(16));
}

#[test]
fn nested_groups_may_extend_the_same_target_but_real_cycles_fail() {
    let tokens = resolve(&[json!({
        "base": {"$type": "number", "t": {"$value": 1}},
        "a": {"$extends": "{base}", "x": {"$extends": "{base}"}}
    })])
    .unwrap();
    for path in ["base.t", "a.t", "a.x.t"] {
        assert_eq!(tokens.get(path).unwrap().value, json!(1), "{path}");
    }

    for (sources, path) in [
        (
            vec![
                json!({"a": {"$extends": "{b}"}}),
                json!({"b": {"$extends": "{a}"}}),
            ],
            "b",
        ),
        (
            vec![
                json!({"a": {"$extends": "{b}"}, "b": {"$extends": "{c}"}, "c": {"$extends": "{a}"}}),
            ],
            "b",
        ),
        (vec![json!({"a": {"$extends": "{a}"}})], "a"),
        (
            vec![
                json!({"g": {"t": {"$type": "number", "$value": 1}, "child": {"$extends": "{g}"}}}),
            ],
            "g.child",
        ),
        (
            vec![json!({
                "g": {"t": {"$type": "number", "$value": 1}, "a": {"$extends": "{h}"}},
                "h": {"b": {"$extends": "{g}"}}
            })],
            "g.a.b",
        ),
        // `b.c` extends `a`, whose expansion is `b`'s content: copies of
        // `b.c` would nest without end, in either authored order.
        (
            vec![json!({
                "a": {"$extends": "{b}"},
                "b": {"t": {"$type": "number", "$value": 1}, "c": {"$extends": "{a}"}}
            })],
            "a.c",
        ),
        (
            vec![json!({
                "b": {"t": {"$type": "number", "$value": 1}, "c": {"$extends": "{a}"}},
                "a": {"$extends": "{b}"}
            })],
            "b.c.c",
        ),
    ] {
        let error = resolve(&sources).unwrap_err();
        assert!(
            matches!(&error, DtcgError::Token { path: reported, message, .. }
                if reported == path && message.starts_with("group extension cycle")),
            "{sources:?}: {error:?}"
        );
    }
}
