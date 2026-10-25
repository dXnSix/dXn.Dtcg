use dxn_dtcg::{DtcgError, DtcgRuntime, selection_from_json, selection_refs};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn number(name: &str, value: i64) -> Value {
    json!({name: {"$type": "number", "$value": value}})
}

/// A root resolver with a `theme` modifier and a nested resolver that
/// declares only a `density` modifier.
fn composed() -> DtcgRuntime {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "modifiers": {"theme": {"contexts": {"light": [number("color", 1)], "dark": [number("color", 2)]}, "default": "light"}},
            "resolutionOrder": [
                {"type": "set", "name": "nested", "sources": [{"$ref": "nested.json"}]},
                {"$ref": "#/modifiers/theme"}
            ]
        })
        .to_string(),
    )
    .unwrap();
    runtime
        .provide_json(
            "nested.json",
            json!({
                "version": "2025.10",
                "modifiers": {"density": {"contexts": {"regular": [number("gap", 8)], "compact": [number("gap", 4)]}}},
                "resolutionOrder": [{"$ref": "#/modifiers/density"}]
            })
            .to_string(),
        )
        .unwrap();
    runtime
}

#[wasm_bindgen_test(unsupported = test)]
fn names_match_case_insensitively_and_normalize_to_the_declared_spelling() {
    let runtime = composed();
    let exact = runtime
        .resolve(&[("theme", "dark"), ("density", "compact")])
        .unwrap()
        .ready()
        .unwrap();
    let folded = runtime
        .resolve(&[("Theme", "DARK"), ("DENSITY", "Compact")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(exact, folded);
    assert_eq!(folded.get("color").unwrap().value, json!(2));
    assert_eq!(folded.get("gap").unwrap().value, json!(4));
    assert_eq!(
        runtime
            .normalize_selection(&[("THEME", "Dark"), ("Density", "COMPACT")])
            .unwrap()
            .ready()
            .unwrap(),
        [
            ("theme".to_owned(), "dark".to_owned()),
            ("density".to_owned(), "compact".to_owned())
        ]
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn ambiguous_case_insensitive_names_are_rejected_unless_exact() {
    let runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "modifiers": {
                "mode": {"contexts": {"dark": [number("a", 1)], "Dark": [number("a", 2)]}, "default": "dark"},
                "MODE": {"contexts": {"x": [], "y": []}, "default": "x"}
            },
            "resolutionOrder": [{"$ref": "#/modifiers/mode"}, {"$ref": "#/modifiers/MODE"}]
        })
        .to_string(),
    )
    .unwrap();
    for result in [
        runtime.resolve(&[("Mode", "dark")]).map(|_| ()),
        runtime.normalize_selection(&[("Mode", "dark")]).map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(DtcgError::UnknownModifier { modifier, known })
                if modifier == "Mode" && known.len() == 2),
            "{result:?}"
        );
    }
    for result in [
        runtime.resolve(&[("mode", "DARK")]).map(|_| ()),
        runtime.normalize_selection(&[("mode", "DARK")]).map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(DtcgError::UnknownContext { context, allowed, .. })
                if context == "DARK" && allowed.len() == 2),
            "{result:?}"
        );
    }
    let exact = runtime
        .resolve(&[("mode", "Dark"), ("MODE", "y")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(exact.get("a").unwrap().value, json!(2));
}

#[wasm_bindgen_test(unsupported = test)]
fn inputs_are_valid_when_any_reached_resolver_declares_them() {
    let runtime = composed();
    // The nested resolver does not declare `theme`, and the root does not
    // declare `density`.
    assert!(
        runtime
            .resolve(&[("theme", "dark"), ("density", "regular")])
            .unwrap()
            .is_ready()
    );
    assert!(
        runtime
            .resolve(&[("density", "compact")])
            .unwrap()
            .is_ready()
    );
    let unknown = runtime
        .resolve(&[("contrast", "high"), ("density", "compact")])
        .unwrap_err();
    assert!(
        matches!(&unknown, DtcgError::UnknownModifier { modifier, known }
            if modifier == "contrast" && *known == ["theme", "density"]),
        "{unknown:?}"
    );
    // While documents are missing, unknown inputs are not reported yet.
    let mut incomplete = composed();
    incomplete.remove_document("nested.json").unwrap();
    assert!(
        incomplete
            .resolve(&[("density", "compact")])
            .unwrap()
            .is_missing()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn each_resolver_applies_its_own_inputs_and_defaults() {
    let runtime = composed();
    // Only the nested resolver declares `density`, which has no default.
    let missing = runtime.resolve(&[("theme", "dark")]).unwrap_err();
    assert!(
        matches!(&missing, DtcgError::UnknownContext { modifier, context, allowed, message }
            if modifier == "density"
                && context.is_empty()
                && *allowed == ["regular", "compact"]
                && message == "modifier has no default and was not selected"),
        "{missing:?}"
    );
    // The root uses its default for `theme`.
    let tokens = runtime
        .resolve(&[("density", "compact")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(tokens.get("color").unwrap().value, json!(1));
    assert_eq!(tokens.get("gap").unwrap().value, json!(4));
}

#[wasm_bindgen_test(unsupported = test)]
fn a_previewed_draft_may_use_its_own_modifiers() {
    let runtime = composed();
    let draft = json!({
        "version": "2025.10",
        "modifiers": {"contrast": {"contexts": {"normal": [number("ink", 1)], "high": [number("ink", 9)]}, "default": "normal"}},
        "resolutionOrder": [{"$ref": "#/modifiers/contrast"}]
    });
    let preview = runtime
        .preview_source(
            &draft,
            &[
                ("contrast", "HIGH"),
                ("theme", "dark"),
                ("density", "regular"),
            ],
        )
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(preview.get("ink").unwrap().value, json!(9));
    assert_eq!(preview.get("color").unwrap().value, json!(2));
    assert!(
        runtime
            .inspect_source(&draft, &[("contrast", "high")])
            .unwrap()
            .is_ready()
    );
    assert!(matches!(
        runtime.preview_source(&draft, &[("unknown", "x"), ("density", "regular")]),
        Err(DtcgError::UnknownModifier { .. })
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn a_graph_without_modifiers_ignores_inputs() {
    let runtime = DtcgRuntime::from_json("tokens.json", number("a", 1).to_string()).unwrap();
    assert!(runtime.resolve(&[("anything", "goes")]).unwrap().is_ready());
    let resolver = DtcgRuntime::from_json(
        "root.json",
        json!({"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [number("a", 1)]}]})
            .to_string(),
    )
    .unwrap();
    assert!(
        resolver
            .resolve(&[("anything", "goes")])
            .unwrap()
            .is_ready()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn json_inputs_become_owned_selections() {
    let runtime = composed();
    let inputs = selection_from_json(&json!({"theme": "dark", "density": "compact"})).unwrap();
    assert_eq!(
        inputs,
        [
            ("theme".to_owned(), "dark".to_owned()),
            ("density".to_owned(), "compact".to_owned())
        ]
    );
    let tokens = runtime
        .resolve(&selection_refs(&inputs))
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(tokens.get("gap").unwrap().value, json!(4));

    for invalid in [
        json!(["theme", "dark"]),
        json!("theme"),
        json!({"theme": 1}),
        json!({"theme": null}),
    ] {
        assert!(
            matches!(
                selection_from_json(&invalid),
                Err(DtcgError::InvalidInput { .. })
            ),
            "{invalid}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn normalize_selection_validates_like_resolve_across_the_graph() {
    let runtime = composed();
    assert_eq!(
        runtime
            .normalize_selection(&[("density", "compact")])
            .unwrap()
            .ready()
            .unwrap(),
        [
            ("theme".to_owned(), "light".to_owned()),
            ("density".to_owned(), "compact".to_owned())
        ]
    );
    for result in [
        runtime.normalize_selection(&[]).map(|_| ()),
        runtime.resolve(&[]).map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(DtcgError::UnknownContext { modifier, message, .. })
                if modifier == "density"
                    && message == "modifier has no default and was not selected"),
            "{result:?}"
        );
    }
    for result in [
        runtime
            .normalize_selection(&[("contrast", "high"), ("density", "compact")])
            .map(|_| ()),
        runtime
            .resolve(&[("contrast", "high"), ("density", "compact")])
            .map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(DtcgError::UnknownModifier { modifier, .. }) if modifier == "contrast"),
            "{result:?}"
        );
    }
    let mut incomplete = composed();
    incomplete.remove_document("nested.json").unwrap();
    assert!(
        incomplete
            .normalize_selection(&[("density", "compact")])
            .unwrap()
            .is_missing()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn inspect_source_accepts_inputs_of_nested_resolvers() {
    let runtime = composed();
    let source = number("extra", 1);
    assert!(
        runtime
            .inspect_source(&source, &[("theme", "dark"), ("density", "compact")])
            .unwrap()
            .is_ready()
    );
    assert!(matches!(
        runtime.inspect_source(&source, &[("contrast", "high"), ("density", "compact")]),
        Err(DtcgError::UnknownModifier { modifier, .. }) if modifier == "contrast"
    ));
    let mut incomplete = composed();
    incomplete.remove_document("nested.json").unwrap();
    assert!(
        incomplete
            .inspect_source(&source, &[("density", "compact")])
            .unwrap()
            .is_missing()
    );
    // Inputs the root resolver declares need no other document.
    assert!(
        incomplete
            .inspect_source(&source, &[("theme", "dark")])
            .unwrap()
            .is_ready()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn previewing_an_occurrence_applies_the_selection_to_the_root_graph_only() {
    let runtime = composed();
    let draft = json!({
        "version": "2025.10",
        "modifiers": {"contrast": {"contexts": {"normal": [number("ink", 1)], "high": [number("ink", 9)]}, "default": "normal"}},
        "resolutionOrder": [{"$ref": "#/modifiers/contrast"}]
    });
    let occurrences = runtime
        .inspect_source(&draft, &[("contrast", "high")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(occurrences.len(), 1);
    assert!(matches!(
        runtime.preview_occurrence(&occurrences[0], &[("contrast", "high"), ("density", "regular")]),
        Err(DtcgError::UnknownModifier { modifier, .. }) if modifier == "contrast"
    ));
    let preview = runtime
        .preview_occurrence(&occurrences[0], &[("density", "regular")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(preview.get("ink").unwrap().value, json!(9));
}

#[wasm_bindgen_test(unsupported = test)]
fn resolve_and_normalize_report_the_same_selection_errors() {
    let runtime = composed();
    let check = |selection: &[(&str, &str)], expected: &dyn Fn(&DtcgError) -> bool| {
        for result in [
            runtime.resolve(selection).map(|_| ()),
            runtime.normalize_selection(selection).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert!(expected(&error), "{selection:?}: {error:?}");
        }
    };
    check(&[("theme", "green"), ("density", "compact")], &|error| {
        matches!(error, DtcgError::UnknownContext { modifier, context, allowed, message }
            if modifier == "theme"
                && context == "green"
                && *allowed == ["light", "dark"]
                && message == "selected modifier context is unknown")
    });
    for twice in [
        [
            ("theme", "dark"),
            ("theme", "light"),
            ("density", "compact"),
        ],
        [
            ("theme", "dark"),
            ("THEME", "light"),
            ("density", "compact"),
        ],
    ] {
        check(&twice, &|error| {
            matches!(error, DtcgError::UnknownContext { modifier, message, .. }
                if modifier == "theme" && message == "modifier is selected twice")
        });
    }
    check(&[("theme", "dark")], &|error| {
        matches!(error, DtcgError::UnknownContext { modifier, allowed, .. }
            if modifier == "density" && *allowed == ["regular", "compact"])
    });
    check(&[("zoom", "in"), ("density", "compact")], &|error| {
        matches!(error, DtcgError::UnknownModifier { modifier, known }
            if modifier == "zoom" && *known == ["theme", "density"])
    });
}
