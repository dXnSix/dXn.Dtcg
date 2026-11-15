// Every selection of a resolver's modifiers (`SelectionSpace`) and the structure check across
// them (`DtcgRuntime::check_structure`): every path missing in some selections or typed
// differently between them, and every selection that fails to resolve.
use dxn_dtcg::{
    DtcgError, DtcgRuntime, Resolution, RuntimeLimits, RuntimeOptions, SelectionSpace,
    StructureReport, TokenType,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn pairs(selection: &[(&str, &str)]) -> Vec<(String, String)> {
    selection
        .iter()
        .map(|(modifier, context)| ((*modifier).to_owned(), (*context).to_owned()))
        .collect()
}

fn runtime(document: Value) -> DtcgRuntime {
    DtcgRuntime::from_json("r.json", document.to_string()).unwrap()
}

fn space(runtime: &DtcgRuntime) -> SelectionSpace {
    runtime.selection_space().unwrap().ready().unwrap()
}

fn check(runtime: &DtcgRuntime) -> StructureReport {
    runtime
        .check_structure(&space(runtime))
        .unwrap()
        .ready()
        .unwrap()
}

fn number(value: f64) -> Value {
    json!({"$type": "number", "$value": value})
}

fn px(value: f64) -> Value {
    json!({"$type": "dimension", "$value": {"value": value, "unit": "px"}})
}

/// `theme` (light, dark) and `density` (comfortable, compact, spacious) over a base set.
fn resolver(dark: Value, compact: Value) -> Value {
    json!({
        "version": "2025.10",
        "sets": {"base": {"sources": [{"gap": number(8.0), "radius": px(4.0)}]}},
        "modifiers": {
            "theme": {"default": "light", "contexts": {"light": [], "dark": [dark]}},
            "density": {"default": "comfortable", "contexts": {
                "comfortable": [], "compact": [compact], "spacious": []
            }}
        },
        "resolutionOrder": [
            {"$ref": "#/sets/base"}, {"$ref": "#/modifiers/theme"}, {"$ref": "#/modifiers/density"}
        ]
    })
}

#[wasm_bindgen_test(unsupported = test)]
fn the_space_enumerates_every_combination_first_axis_slowest() {
    let runtime = runtime(resolver(json!({}), json!({})));
    let space = space(&runtime);
    let names = space
        .axes()
        .iter()
        .map(|axis| axis.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["theme", "density"]);
    assert_eq!(space.combinations(), Some(6));
    let selections = space.iter().collect::<Vec<_>>();
    assert_eq!(
        selections,
        [
            pairs(&[("theme", "light"), ("density", "comfortable")]),
            pairs(&[("theme", "light"), ("density", "compact")]),
            pairs(&[("theme", "light"), ("density", "spacious")]),
            pairs(&[("theme", "dark"), ("density", "comfortable")]),
            pairs(&[("theme", "dark"), ("density", "compact")]),
            pairs(&[("theme", "dark"), ("density", "spacious")]),
        ]
    );
    // `&SelectionSpace` is iterable, and the iterator can be restarted.
    assert_eq!((&space).into_iter().count(), 6);
    assert_eq!(space.iter().count(), 6);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_token_document_has_one_empty_selection() {
    let runtime = runtime(json!({"gap": number(8.0)}));
    let space = space(&runtime);
    assert!(space.axes().is_empty());
    assert_eq!(space.combinations(), Some(1));
    assert_eq!(space.iter().collect::<Vec<_>>(), [Vec::new()]);
    let report = check(&runtime);
    assert!(report.is_consistent());
    assert_eq!(report.selections, [Vec::new()]);
}

#[wasm_bindgen_test(unsupported = test)]
fn consistent_contexts_report_no_difference() {
    let runtime = runtime(resolver(
        json!({"gap": number(10.0)}),
        json!({"radius": px(2.0)}),
    ));
    let report = check(&runtime);
    assert!(report.is_consistent(), "{report:?}");
    assert_eq!(report.selections.len(), 6);
    assert!(report.differences.is_empty() && report.failures.is_empty());
}

#[wasm_bindgen_test(unsupported = test)]
fn every_missing_path_and_type_change_is_reported() {
    let runtime = runtime(resolver(
        // Dark retypes `gap` and adds a token.
        json!({"gap": px(10.0), "shadow": {"tint": number(0.5)}}),
        // Compact adds a token too.
        json!({"tight": number(1.0)}),
    ));
    let report = check(&runtime);
    assert!(!report.is_consistent());
    assert!(report.failures.is_empty());
    assert_eq!(
        report.selections,
        space(&runtime).iter().collect::<Vec<_>>()
    );

    let paths = report
        .differences
        .iter()
        .map(|difference| difference.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["gap", "shadow.tint", "tight"]);

    let [gap, tint, tight] = report.differences.as_slice() else {
        panic!()
    };
    let (number, dimension) = (Some(TokenType::Number), Some(TokenType::Dimension));
    assert_eq!(
        gap.types,
        [number, number, number, dimension, dimension, dimension]
    );
    assert!(gap.missing_in().is_empty());
    assert_eq!(
        gap.distinct_types(),
        [TokenType::Number, TokenType::Dimension]
    );

    assert_eq!(tint.types, [None, None, None, number, number, number]);
    assert_eq!(tint.missing_in(), [0, 1, 2]);
    assert_eq!(tint.distinct_types(), [TokenType::Number]);

    assert_eq!(tight.types, [None, number, None, None, number, None]);
    assert_eq!(tight.missing_in(), [0, 2, 3, 5]);
}

#[wasm_bindgen_test(unsupported = test)]
fn failing_selections_are_reported_and_left_out_of_the_comparison() {
    let runtime = runtime(resolver(
        json!({"gap": {"$type": "number", "$value": "{missing}"}}),
        json!({"tight": number(1.0)}),
    ));
    let report = check(&runtime);
    assert_eq!(report.failures.len(), 3);
    for failure in &report.failures {
        assert_eq!(failure.selection[0], ("theme".into(), "dark".into()));
        assert!(
            matches!(&failure.error, DtcgError::UnresolvedReference { path, .. } if path == "gap"),
            "{:?}",
            failure.error
        );
    }
    assert_eq!(
        report.selections,
        [
            pairs(&[("theme", "light"), ("density", "comfortable")]),
            pairs(&[("theme", "light"), ("density", "compact")]),
            pairs(&[("theme", "light"), ("density", "spacious")]),
        ]
    );
    assert_eq!(report.differences.len(), 1);
    assert_eq!(report.differences[0].path, "tight");
    assert_eq!(report.differences[0].missing_in(), [0, 2]);
}

#[wasm_bindgen_test(unsupported = test)]
fn any_list_of_selections_can_be_checked() {
    let runtime = runtime(resolver(json!({"only": number(1.0)}), json!({})));
    let report = runtime
        .check_structure(vec![
            pairs(&[("theme", "dark")]),
            pairs(&[]),
            pairs(&[("Theme", "DARK"), ("density", "compact")]),
            pairs(&[("theme", "sepia")]),
        ])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(report.selections.len(), 3);
    assert_eq!(report.differences.len(), 1);
    assert_eq!(
        report.differences[0].types,
        [Some(TokenType::Number), None, Some(TokenType::Number)]
    );
    assert_eq!(report.failures.len(), 1);
    assert!(matches!(
        report.failures[0].error,
        DtcgError::UnknownContext { .. }
    ));

    let empty = runtime
        .check_structure(Vec::new())
        .unwrap()
        .ready()
        .unwrap();
    assert!(empty.is_consistent() && empty.selections.is_empty());
}

#[wasm_bindgen_test(unsupported = test)]
fn the_selection_limit_is_checked_before_resolving() {
    let document = resolver(json!({}), json!({}));
    let limited = DtcgRuntime::from_json_with_options(
        "r.json",
        document.to_string(),
        RuntimeOptions::default().with_limits(RuntimeLimits::default().with_max_selections(5)),
    )
    .unwrap();
    assert_eq!(RuntimeLimits::default().max_selections, 1024);
    assert_eq!(
        limited.check_structure(&space(&limited)).unwrap_err(),
        DtcgError::LimitExceeded {
            kind: "selections",
            limit: 5
        }
    );
    let report = limited
        .check_structure(space(&limited).iter().take(5))
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(report.selections.len(), 5);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_combinatorial_space_is_lazy_and_capped() {
    // 64 modifiers of two contexts: 2^64 selections, more than a usize counts.
    let mut modifiers = serde_json::Map::new();
    let mut order = Vec::new();
    for index in 0..64 {
        let name = format!("m{index}");
        modifiers.insert(
            name.clone(),
            json!({"default": "a", "contexts": {"a": [], "b": []}}),
        );
        order.push(json!({"$ref": format!("#/modifiers/{name}")}));
    }
    let runtime = runtime(json!({
        "version": "2025.10",
        "modifiers": modifiers,
        "resolutionOrder": order,
    }));
    let space = space(&runtime);
    assert_eq!(space.axes().len(), 64);
    assert_eq!(space.combinations(), None);
    assert_eq!(
        space.iter().nth(3).unwrap()[62..],
        pairs(&[("m62", "b"), ("m63", "b")])
    );
    assert!(matches!(
        runtime.check_structure(&space),
        Err(DtcgError::LimitExceeded {
            kind: "selections",
            limit: 1024
        })
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn documents_any_selection_needs_are_requested() {
    let mut runtime = runtime(json!({
        "version": "2025.10",
        "modifiers": {"theme": {"default": "light", "contexts": {
            "light": [{"$ref": "light.json"}],
            "dark": [{"$ref": "dark.json"}]
        }}},
        "resolutionOrder": [{"$ref": "#/modifiers/theme"}]
    }));
    let space = space(&runtime);
    let Resolution::Missing(requests) = runtime.check_structure(&space).unwrap() else {
        panic!()
    };
    let ids = requests
        .iter()
        .map(|request| request.resource_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["dark.json", "light.json"]);

    runtime
        .provide_json("light.json", json!({"bg": number(0.0)}).to_string())
        .unwrap();
    runtime
        .provide_json("dark.json", json!({"bg": number(1.0)}).to_string())
        .unwrap();
    assert!(
        runtime
            .check_structure(&space)
            .unwrap()
            .ready()
            .unwrap()
            .is_consistent()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn nested_resolvers_follow_shared_inputs_and_their_defaults() {
    let mut runtime = runtime(json!({
        "version": "2025.10",
        "sets": {"brand": {"sources": [{"$ref": "brand.resolver.json"}]}},
        "modifiers": {"theme": {"default": "light", "contexts": {"light": [], "dark": []}}},
        "resolutionOrder": [{"$ref": "#/sets/brand"}, {"$ref": "#/modifiers/theme"}]
    }));
    runtime
        .provide_json(
            "brand.resolver.json",
            json!({
                "version": "2025.10",
                "modifiers": {
                    "theme": {"default": "light", "contexts": {
                        "light": [], "dark": [{"accent": number(2.0)}]
                    }},
                    "contrast": {"default": "normal", "contexts": {
                        "normal": [], "high": [{"outline": number(1.0)}]
                    }}
                },
                "resolutionOrder": [{"$ref": "#/modifiers/theme"}, {"$ref": "#/modifiers/contrast"}]
            })
            .to_string(),
        )
        .unwrap();
    let space = space(&runtime);
    // Only the root's modifiers are axes; `contrast` keeps its default.
    assert_eq!(space.combinations(), Some(2));
    let report = runtime.check_structure(&space).unwrap().ready().unwrap();
    assert!(report.failures.is_empty());
    assert_eq!(report.differences.len(), 1);
    assert_eq!(report.differences[0].path, "accent");
    assert_eq!(report.differences[0].missing_in(), [0]);
}
