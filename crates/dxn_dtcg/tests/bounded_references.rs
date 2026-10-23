use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution, RuntimeLimits};
use serde_json::{Map, Value, json};

fn runtime(document: &Value) -> DtcgRuntime {
    DtcgRuntime::from_json("tokens.json", document.to_string()).unwrap()
}

fn resolve(document: &Value) -> Result<Resolution<dxn_dtcg::ResolvedTokens>, DtcgError> {
    runtime(document).resolve(&[])
}

fn limited(limits: RuntimeLimits, document: &Value) -> DtcgRuntime {
    let mut runtime = DtcgRuntime::with_limits(limits);
    runtime
        .provide_json("tokens.json", document.to_string())
        .unwrap();
    runtime.set_root("tokens.json").unwrap();
    runtime
}

fn is_limit(result: &Result<impl std::fmt::Debug, DtcgError>, expected: &str) -> bool {
    matches!(result, Err(DtcgError::LimitExceeded { kind, .. }) if *kind == expected)
}

/// Runs `body` on a thread with a 1 MiB stack, so unbounded recursion would
/// overflow it.
fn on_small_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(body)
        .unwrap()
        .join()
        .unwrap()
}

/// `$extensions` members `p0 = 1` and `p<i> = { "$ref": "#/$extensions/p<i-1>" }`.
fn pointer_chain(length: usize) -> Value {
    let mut extensions = Map::new();
    extensions.insert("p0".into(), json!(1));
    for index in 1..length {
        extensions.insert(
            format!("p{index}"),
            json!({"$ref": format!("#/$extensions/p{}", index - 1)}),
        );
    }
    json!({
        "$extensions": extensions,
        "answer": {"$type": "number", "$value": {"$ref": format!("#/$extensions/p{}", length - 1)}}
    })
}

#[test]
fn a_hundred_thousand_document_pointer_hops_hit_the_alias_depth_without_overflowing() {
    let document = pointer_chain(100_000);
    let result = on_small_stack(move || resolve(&document));
    assert!(is_limit(&result, "alias depth"), "{result:?}");
}

#[test]
fn dependency_collection_counts_document_pointer_hops() {
    let document = pointer_chain(100_000);
    let result = on_small_stack(move || {
        let mut runtime = runtime(&json!({"answer": {"$type": "number", "$value": 1}}));
        runtime.replace_json_and_resolve_affected(
            "tokens.json",
            document.to_string(),
            &[],
            &["answer"],
        )
    });
    assert!(is_limit(&result, "alias depth"), "{result:?}");
}

#[test]
fn a_hundred_thousand_alias_hops_hit_the_alias_depth_without_overflowing() {
    let mut document = Map::new();
    for index in (1..100_000).rev() {
        document.insert(
            format!("t{index}"),
            json!({"$type": "number", "$value": format!("{{t{}}}", index - 1)}),
        );
    }
    document.insert("t0".into(), json!({"$type": "number", "$value": 0}));
    let document = Value::Object(document);
    let result = on_small_stack(move || resolve(&document));
    assert!(is_limit(&result, "alias depth"), "{result:?}");
}

#[test]
fn duplicated_pointer_fan_out_is_stopped_by_the_value_node_limit() {
    let mut extensions = Map::new();
    extensions.insert("l0".into(), json!(1));
    for level in 1..40 {
        let previous = json!({"$ref": format!("#/$extensions/l{}", level - 1)});
        extensions.insert(format!("l{level}"), json!([previous, previous]));
    }
    let document = json!({
        "$extensions": extensions,
        "t": {"$type": "number", "$value": {"$ref": "#/$extensions/l39"}}
    });
    assert!(document.to_string().len() < 3_000);
    let result = resolve(&document);
    assert!(is_limit(&result, "value nodes"), "{result:?}");

    let small = limited(
        RuntimeLimits::default().with_max_value_nodes(3),
        &json!({
            "a": {"$type": "cubicBezier", "$value": [0, 0, 1, 1]},
            "b": {"$type": "cubicBezier", "$value": "{a}"}
        }),
    );
    assert!(is_limit(&small.resolve(&[]), "value nodes"));
    let enough = limited(
        RuntimeLimits::default().with_max_value_nodes(5),
        &json!({
            "a": {"$type": "cubicBezier", "$value": [0, 0, 1, 1]},
            "b": {"$type": "cubicBezier", "$value": "{a}"}
        }),
    );
    assert!(enough.resolve(&[]).unwrap().is_ready());
}

#[test]
fn equal_pointer_text_in_different_sources_is_not_a_cycle_in_either_order() {
    // Sources resolve as one merged structure, so `#/$extensions/x` names
    // the later source's value; the pointer is never mistaken for a cycle
    // while it resolves through an alias.
    let first = json!({
        "$extensions": {"x": "{c}"},
        "c": {"$type": "number", "$value": 5},
        "a": {"$type": "number", "$value": {"$ref": "#/$extensions/x"}}
    });
    let second = json!({
        "$extensions": {"x": 2},
        "b": {"$type": "number", "$value": {"$ref": "#/$extensions/x"}}
    });
    for (sources, expected) in [
        (vec![first.clone(), second.clone()], json!(2)),
        (vec![second.clone(), first.clone()], json!(5)),
    ] {
        let runtime = runtime(&json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "all", "sources": sources}]
        }));
        let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
        assert_eq!(tokens.get("a").unwrap().value, expected);
        assert_eq!(tokens.get("b").unwrap().value, expected);
    }
}

#[test]
fn a_sibling_override_may_point_at_its_own_target() {
    let document = json!({
        "$extensions": {"size": {"value": 4, "unit": "px"}},
        "nested": {
            "$type": "dimension",
            "$value": {"$ref": "#/$extensions/size", "unit": {"$ref": "#/$extensions/size"}}
        }
    });
    let error = resolve(&document).unwrap_err();
    assert!(
        matches!(&error, DtcgError::TokenType { path, .. } if path == "nested"),
        "the override resolves (to a non-dimension value) instead of being circular: {error:?}"
    );

    let valid = json!({
        "$extensions": {"size": {"value": 4, "unit": "px"}, "unit": "rem"},
        "token": {
            "$type": "dimension",
            "$value": {"$ref": "#/$extensions/size", "unit": {"$ref": "#/$extensions/unit"}}
        }
    });
    let tokens = resolve(&valid).unwrap().ready().unwrap();
    assert_eq!(
        tokens.get("token").unwrap().value,
        json!({"value": 4, "unit": "rem"})
    );
}

#[test]
fn alias_depth_zero_resolves_literal_tokens_and_rejects_the_first_alias() {
    let literal = limited(
        RuntimeLimits::default().with_max_alias_depth(0),
        &json!({"a": {"$type": "number", "$value": 1}, "b": {"$type": "number", "$value": 2}}),
    );
    assert_eq!(literal.resolve(&[]).unwrap().ready().unwrap().len(), 2);

    let alias = limited(
        RuntimeLimits::default().with_max_alias_depth(0),
        &json!({"a": {"$type": "number", "$value": 1}, "b": {"$type": "number", "$value": "{a}"}}),
    );
    assert!(is_limit(&alias.resolve(&[]), "alias depth"));
}

/// A chain of `hops` references from `t<hops>` down to the literal `t0`,
/// mixing curly aliases, token pointers and document pointers.
fn chain(hops: usize, reverse: bool) -> Value {
    let mut entries = vec![("t0".to_owned(), json!({"$type": "number", "$value": 0}))];
    for index in 1..=hops {
        let previous = format!("t{}", index - 1);
        let value = match index % 3 {
            0 => json!(format!("{{{previous}}}")),
            1 => json!({"$ref": format!("#/{previous}/$value")}),
            _ => json!({"$ref": format!("#/$extensions/to{index}")}),
        };
        entries.push((
            format!("t{index}"),
            json!({"$type": "number", "$value": value}),
        ));
    }
    let mut extensions = Map::new();
    for index in 1..=hops {
        if index % 3 == 2 {
            extensions.insert(format!("to{index}"), json!(format!("{{t{}}}", index - 1)));
        }
    }
    if reverse {
        entries.reverse();
    }
    let mut document = Map::from_iter(entries);
    document.insert("$extensions".into(), Value::Object(extensions));
    Value::Object(document)
}

/// Hops of the mixed chain with `links` links: a document-pointer link reads
/// an `$extensions` member that holds an alias, so it costs two hops.
fn chain_hops(links: usize) -> usize {
    (1..=links)
        .map(|index| if index % 3 == 2 { 2 } else { 1 })
        .sum()
}

#[test]
fn alias_depth_counts_every_reference_hop_in_any_authored_order() {
    for links in [1, 2, 3, 5, 8] {
        let hops = chain_hops(links);
        for reverse in [false, true] {
            let within = limited(
                RuntimeLimits::default().with_max_alias_depth(hops),
                &chain(links, reverse),
            );
            assert!(
                within.resolve(&[]).unwrap().is_ready(),
                "{hops} hops within a limit of {hops} (reverse: {reverse})"
            );
            let beyond = limited(
                RuntimeLimits::default().with_max_alias_depth(hops - 1),
                &chain(links, reverse),
            );
            assert!(
                is_limit(&beyond.resolve(&[]), "alias depth"),
                "{hops} hops beyond a limit of {} (reverse: {reverse})",
                hops - 1
            );
        }
    }
}

#[test]
fn plain_alias_chains_of_n_hops_resolve_and_n_plus_one_fail() {
    let chain = |hops: usize, reverse: bool| {
        let mut entries = vec![("t0".to_owned(), json!({"$type": "number", "$value": 0}))];
        for index in 1..=hops {
            entries.push((
                format!("t{index}"),
                json!({"$type": "number", "$value": format!("{{t{}}}", index - 1)}),
            ));
        }
        if reverse {
            entries.reverse();
        }
        Value::Object(Map::from_iter(entries))
    };
    for limit in [1, 4, 64] {
        for reverse in [false, true] {
            let limits = RuntimeLimits::default().with_max_alias_depth(limit);
            assert!(
                limited(limits.clone(), &chain(limit, reverse))
                    .resolve(&[])
                    .unwrap()
                    .is_ready()
            );
            assert!(is_limit(
                &limited(limits, &chain(limit + 1, reverse)).resolve(&[]),
                "alias depth"
            ));
        }
    }
}

#[test]
fn array_indexes_follow_rfc_6901_in_token_and_document_pointers() {
    let document = |pointer: &str| {
        json!({
            "$extensions": {"list": [10, 20]},
            "source": {"$type": "cubicBezier", "$value": [0.1, 0.2, 0.3, 0.4]},
            "target": {"$type": "number", "$value": {"$ref": pointer}}
        })
    };
    for (pointer, expected) in [
        ("#/source/$value/0", json!(0.1)),
        ("#/source/$value/1", json!(0.2)),
        ("#/$extensions/list/0", json!(10)),
        ("#/$extensions/list/1", json!(20)),
    ] {
        let tokens = resolve(&document(pointer)).unwrap().ready().unwrap();
        assert_eq!(tokens.get("target").unwrap().value, expected, "{pointer}");
    }
    for index in ["+1", "01", "-1", "4", "1.0", " 1", ""] {
        for prefix in ["#/source/$value/", "#/$extensions/list/"] {
            let pointer = format!("{prefix}{index}");
            let error = resolve(&document(&pointer)).unwrap_err();
            assert!(
                matches!(&error, DtcgError::InvalidPointer(reported) if reported == &pointer),
                "{pointer}: {error:?}"
            );
        }
    }
}

#[test]
fn percent_encoded_pointers_and_fragments_are_decoded() {
    let tokens = resolve(&json!({
        "$extensions": {"brand colors": 7, "a/b": 8, "c~d": 9},
        "spaced": {"$type": "number", "$value": {"$ref": "#/$extensions/brand%20colors"}},
        "slash": {"$type": "number", "$value": {"$ref": "#/$extensions/a~1b"}},
        "tilde": {"$type": "number", "$value": {"$ref": "#/$extensions/c%7E0d"}}
    }))
    .unwrap()
    .ready()
    .unwrap();
    assert_eq!(tokens.get("spaced").unwrap().value, json!(7));
    assert_eq!(tokens.get("slash").unwrap().value, json!(8));
    assert_eq!(tokens.get("tilde").unwrap().value, json!(9));

    let mut layered = runtime(&json!({
        "version": "2025.10",
        "resolutionOrder": [{"type": "set", "name": "lib", "sources": [{"$ref": "lib.json#/brand%20colors"}]}]
    }));
    layered
        .provide_json(
            "lib.json",
            json!({"brand colors": {"primary": {"$type": "number", "$value": 3}}}).to_string(),
        )
        .unwrap();
    let tokens = layered.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("primary").unwrap().value, json!(3));
    let references = layered.source_references().unwrap().ready().unwrap();
    assert_eq!(
        references[0].reference().fragment(),
        Some("/brand%20colors")
    );

    for malformed in [
        "#/$extensions/%2",
        "#/$extensions/%zz",
        "#/$extensions/%FF",
        "#/a~2",
    ] {
        let error = resolve(&json!({
            "$extensions": {"x": 1},
            "t": {"$type": "number", "$value": {"$ref": malformed}}
        }))
        .unwrap_err();
        assert!(
            matches!(error, DtcgError::InvalidPointer(_)),
            "{malformed}: {error:?}"
        );
    }
}

#[test]
fn malformed_corpus_pointers_stay_rejected() {
    for pointer in [
        "#/colors//primary/$value",
        "#/colors.primary.$value",
        "/colors/primary/$value",
        "#/colors/color/$value/components/-1",
        "#/colors/color/$value/components/99",
        "#/colors/color/$value/components/first",
    ] {
        let error = resolve(&json!({
            "colors": {
                "primary": {"$type": "number", "$value": 1},
                "color": {"$type": "color", "$value": {"colorSpace": "srgb", "components": [1, 0, 0]}}
            },
            "t": {"$type": "number", "$value": {"$ref": pointer}}
        }));
        assert!(error.is_err(), "{pointer} must be rejected");
    }
}

/// `$extensions` members `n0` … `n<hops-1>`, each nested `levels` arrays deep
/// around a pointer to the previous member.
fn nested_pointer_chain(hops: usize, levels: usize) -> Value {
    let mut extensions = Map::new();
    for index in 0..hops {
        let mut value = if index == 0 {
            json!(1)
        } else {
            json!({"$ref": format!("#/$extensions/n{}", index - 1)})
        };
        for _ in 0..levels {
            value = json!([value]);
        }
        extensions.insert(format!("n{index}"), value);
    }
    json!({
        "$extensions": extensions,
        "deep": {"$type": "number", "$value": {"$ref": format!("#/$extensions/n{}", hops - 1)}}
    })
}

#[test]
fn nesting_that_adds_up_across_references_hits_the_value_depth_on_a_small_stack() {
    let document = nested_pointer_chain(20, 120);
    let resolved = {
        let document = document.clone();
        on_small_stack(move || resolve(&document).map(|_| ()))
    };
    assert!(is_limit(&resolved, "value depth"), "{resolved:?}");

    let collected = on_small_stack(move || {
        let mut runtime = runtime(&json!({"deep": {"$type": "number", "$value": 1}}));
        runtime
            .replace_json_and_resolve_affected("tokens.json", document.to_string(), &[], &["deep"])
            .map(|_| ())
    });
    assert!(is_limit(&collected, "value depth"), "{collected:?}");

    // An alias chain nests the same way; the deepest token comes first so it
    // is resolved before any value is validated. The tokens are untyped, so
    // the aliases inside their arrays are not position-checked.
    let mut aliases = Map::new();
    for index in (0..3).rev() {
        let mut value = if index == 0 {
            json!(1)
        } else {
            json!(format!("{{t{}}}", index - 1))
        };
        for _ in 0..60 {
            value = json!([value]);
        }
        aliases.insert(format!("t{index}"), json!({"$value": value}));
    }
    let aliases = Value::Object(aliases);
    let result = on_small_stack(move || resolve(&aliases).map(|_| ()));
    assert!(is_limit(&result, "value depth"), "{result:?}");
}

#[test]
fn nesting_within_the_value_depth_is_resolved() {
    // Two hops of 40 levels each nest 81 levels deep: resolution succeeds and
    // only the number type rejects the array.
    let error = resolve(&nested_pointer_chain(2, 40)).unwrap_err();
    assert!(
        matches!(&error, DtcgError::TokenType { path, .. } if path == "deep"),
        "{error:?}"
    );
}

#[test]
fn percent_encoded_references_into_resolution_order_or_modifiers_stay_forbidden() {
    for (reference, inline_modifier) in [
        ("#/%72esolutionOrder/0", false),
        ("#/resolution%4Frder/0", false),
        ("#/%6Dodifiers/m", true),
        ("#/modifiers/m", true),
    ] {
        let mut resolver = json!({
            "version": "2025.10",
            "sets": {"base": {"sources": [{"$ref": reference}]}},
            "resolutionOrder": [{"$ref": "#/sets/base"}]
        });
        if inline_modifier {
            resolver["modifiers"] = json!({"m": {"contexts": {"a": [], "b": []}, "default": "a"}});
        }
        let runtime = runtime(&resolver);
        for result in [
            runtime.resolve(&[]).map(|_| ()),
            runtime.validate_complete().map(|_| ()),
        ] {
            assert!(
                matches!(&result, Err(DtcgError::InvalidDocument { message, .. })
                    if message.contains("forbidden source reference")),
                "{reference}: {result:?}"
            );
        }
    }
}

#[test]
fn token_pointer_copies_are_charged_before_they_are_made() {
    let document = json!({
        "a": {"$type": "cubicBezier", "$value": [0, 0, 1, 1]},
        "b": {"$type": "cubicBezier", "$value": {"$ref": "#/a/$value"}}
    });
    let below = limited(RuntimeLimits::default().with_max_value_nodes(4), &document);
    assert!(is_limit(&below.resolve(&[]), "value nodes"));
    let exact = limited(RuntimeLimits::default().with_max_value_nodes(5), &document);
    assert!(exact.resolve(&[]).unwrap().is_ready());
}
