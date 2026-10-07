use dtcg_runtime::{AffectedResolution, DtcgError, DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

fn number(value: i64) -> Value {
    json!({"$type": "number", "$value": value})
}

fn resolve(runtime: &DtcgRuntime) -> ResolvedTokens {
    runtime.resolve(&[]).unwrap().ready().unwrap()
}

/// Edits `resource` from its current content to `edited`, applies the
/// affected result to the snapshot taken before, and checks that it equals a
/// full resolution afterwards.
fn edit(
    runtime: &mut DtcgRuntime,
    resource: &str,
    edited: &Value,
    changed_paths: &[&str],
) -> AffectedResolution {
    let mut snapshot = resolve(runtime);
    let affected = runtime
        .replace_json_and_resolve_affected(resource, edited.to_string(), &[], changed_paths)
        .unwrap()
        .ready()
        .unwrap();
    snapshot.apply_affected(&affected);
    let full = resolve(runtime);
    assert_eq!(snapshot, full, "patched snapshot after {edited}");
    assert_eq!(snapshot.fingerprint(), full.fingerprint());
    assert_eq!(
        affected.order(),
        full.iter()
            .map(|token| token.path.to_string())
            .collect::<Vec<_>>()
    );
    affected
}

fn affected_paths(affected: &AffectedResolution) -> Vec<&str> {
    affected.iter().map(|token| token.path.as_ref()).collect()
}

fn single(document: &Value) -> DtcgRuntime {
    DtcgRuntime::from_json("tokens.json", document.to_string()).unwrap()
}

fn layered(base: &Value, overrides: &Value) -> DtcgRuntime {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "all", "sources": [{"$ref": "base.json"}, {"$ref": "overrides.json"}]}]
        })
        .to_string(),
    )
    .unwrap();
    runtime.provide_json("base.json", base.to_string()).unwrap();
    runtime
        .provide_json("overrides.json", overrides.to_string())
        .unwrap();
    runtime
}

#[test]
fn deleting_an_aliased_token_is_rejected_and_rolled_back() {
    let initial = json!({"a": number(1), "b": {"$type": "number", "$value": "{a}"}});
    let mut runtime = single(&initial);
    let before = resolve(&runtime);
    let error = runtime
        .replace_json_and_resolve_affected(
            "tokens.json",
            json!({"b": {"$type": "number", "$value": "{a}"}}).to_string(),
            &[],
            &["a"],
        )
        .unwrap_err();
    assert!(
        matches!(&error, DtcgError::UnresolvedReference { path, target, .. }
            if path == "b" && target == "a"),
        "{error:?}"
    );
    assert_eq!(resolve(&runtime), before);

    // A broken dependent is found without naming any path.
    let error = runtime
        .replace_json_and_resolve_affected(
            "tokens.json",
            json!({"b": {"$type": "number", "$value": "{a}"}}).to_string(),
            &[],
            &[],
        )
        .unwrap_err();
    assert!(matches!(error, DtcgError::UnresolvedReference { .. }));
    assert_eq!(resolve(&runtime), before);
}

#[test]
fn every_single_document_edit_patches_to_the_full_resolution() {
    let initial = json!({
        "a": number(1),
        "b": {"$type": "number", "$value": "{a}"},
        "c": number(3),
        "d": number(4)
    });
    let mut runtime = single(&initial);

    // A value change re-resolves its dependents.
    let affected = edit(
        &mut runtime,
        "tokens.json",
        &json!({"a": number(2), "b": {"$type": "number", "$value": "{a}"}, "c": number(3), "d": number(4)}),
        &["a"],
    );
    assert_eq!(affected_paths(&affected), ["a", "b"]);

    // An addition.
    let affected = edit(
        &mut runtime,
        "tokens.json",
        &json!({"a": number(2), "b": {"$type": "number", "$value": "{a}"}, "c": number(3), "d": number(4), "e": number(5)}),
        &["e"],
    );
    assert_eq!(affected_paths(&affected), ["e"]);

    // A deletion of a token nothing aliases.
    let affected = edit(
        &mut runtime,
        "tokens.json",
        &json!({"a": number(2), "b": {"$type": "number", "$value": "{a}"}, "d": number(4), "e": number(5)}),
        &["c"],
    );
    assert_eq!(affected.removed(), ["c"]);
    assert!(affected_paths(&affected).is_empty());

    // A move that changes the order, without naming any path.
    let affected = edit(
        &mut runtime,
        "tokens.json",
        &json!({"e": number(5), "a": number(2), "b": {"$type": "number", "$value": "{a}"}, "d": number(4)}),
        &[],
    );
    assert_eq!(affected.order(), ["e", "a", "b", "d"]);
}

#[test]
fn overrides_in_another_document_patch_to_the_full_resolution() {
    let base = json!({"size": {"$type": "number", "sm": {"$value": 4}, "lg": {"$value": 16}}, "alias": {"$type": "number", "$value": "{size.lg}"}});
    let mut runtime = layered(&base, &json!({}));
    let affected = edit(
        &mut runtime,
        "overrides.json",
        &json!({"size": {"lg": {"$value": 12}}}),
        &["size.lg"],
    );
    assert_eq!(affected_paths(&affected), ["alias", "size.lg"]);
    assert_eq!(
        resolve(&runtime).get("size.lg").unwrap().source.as_str(),
        "overrides.json"
    );

    // Removing the override restores the base definition.
    let affected = edit(&mut runtime, "overrides.json", &json!({}), &["size.lg"]);
    assert!(affected.removed().is_empty());
    assert_eq!(resolve(&runtime).get("size.lg").unwrap().value, json!(16));
}

#[test]
fn naming_a_group_selects_every_token_under_it() {
    let initial = json!({"g": {"$type": "number", "t": {"$value": 400}, "inner": {"u": {"$value": 500}}}, "other": number(1)});
    let mut runtime = single(&initial);
    let affected = edit(
        &mut runtime,
        "tokens.json",
        &json!({"g": {"$type": "fontWeight", "$deprecated": true, "t": {"$value": 400}, "inner": {"u": {"$value": 500}}}, "other": number(1)}),
        &["g"],
    );
    assert_eq!(affected_paths(&affected), ["g.t", "g.inner.u"]);
    let tokens = resolve(&runtime);
    assert_eq!(
        tokens.get("g.t").unwrap().token_type.as_deref(),
        Some("fontWeight")
    );
    assert!(tokens.get("g.inner.u").unwrap().deprecated.is_deprecated());

    // `$extends` edits take effect as well.
    let mut runtime = single(&json!({
        "base": {"$type": "number", "x": {"$value": 1}},
        "alt": {"$type": "number", "x": {"$value": 2}},
        "theme": {"$extends": "{base}"}
    }));
    edit(
        &mut runtime,
        "tokens.json",
        &json!({
            "base": {"$type": "number", "x": {"$value": 1}},
            "alt": {"$type": "number", "x": {"$value": 2}},
            "theme": {"$extends": "{alt}"}
        }),
        &["theme"],
    );
    assert_eq!(resolve(&runtime).get("theme.x").unwrap().value, json!(2));
}

#[test]
fn readers_of_document_pointers_are_always_affected() {
    let document = |value: i64| {
        json!({
            "$extensions": {"palette": {"x": value}},
            "reader": {"$type": "number", "$value": {"$ref": "#/$extensions/palette/x"}},
            "plain": number(1)
        })
    };
    let mut runtime = single(&document(1));
    let affected = edit(&mut runtime, "tokens.json", &document(2), &[]);
    assert_eq!(affected_paths(&affected), ["reader"]);
    assert_eq!(resolve(&runtime).get("reader").unwrap().value, json!(2));
}

#[test]
fn only_real_group_extends_members_force_a_full_re_resolution() {
    let document = |value: i64| {
        json!({
            "$extensions": {"vendor": {"$extends": "not a group member"}},
            "a": number(value),
            "b": number(2),
            "c": {"$type": "number", "$value": 3, "$extensions": {"$extends": "{a}"}}
        })
    };
    let mut runtime = single(&document(1));
    let affected = edit(&mut runtime, "tokens.json", &document(5), &["a"]);
    assert_eq!(affected_paths(&affected), ["a"]);

    let mut extending = single(
        &json!({"base": {"$type": "number", "x": {"$value": 1}}, "theme": {"$extends": "{base}"}, "a": number(1)}),
    );
    let affected = edit(
        &mut extending,
        "tokens.json",
        &json!({"base": {"$type": "number", "x": {"$value": 1}}, "theme": {"$extends": "{base}"}, "a": number(2)}),
        &["a"],
    );
    assert_eq!(affected.iter().count(), 3);
}
