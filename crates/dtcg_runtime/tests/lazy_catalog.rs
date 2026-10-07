use std::sync::Arc;

use dtcg_runtime::{DtcgRuntime, Resolution, RuntimeLimits};

const ROOT: &str = r##"{
  "version": "2025.10",
  "resolutionOrder": [{"$ref":"#/sets/base"}, {"$ref":"#/modifiers/theme"}],
  "sets": {"base": {"sources": [{"$ref":"tokens/base.json"}]}},
  "modifiers": {"theme": {"default":"light", "contexts": {
    "light": [{"$ref":"tokens/light.json"}], "dark": [{"$ref":"tokens/dark.json"}]
  }}}
}"##;

#[test]
fn lazy_resolution_returns_the_stable_active_missing_frontier_then_converges() {
    let mut runtime = DtcgRuntime::from_json("workspace/root.json", ROOT).unwrap();

    let requests = match runtime.resolve(&[("theme", "dark")]).unwrap() {
        Resolution::Missing(requests) => requests,
        Resolution::Ready(_) => panic!("missing active documents must be requested"),
    };
    assert_eq!(
        requests
            .iter()
            .map(|request| request.resource_id.as_str())
            .collect::<Vec<_>>(),
        ["workspace/tokens/base.json", "workspace/tokens/dark.json"]
    );
    assert!(
        requests
            .iter()
            .all(|request| request.original_reference.starts_with("tokens/"))
    );

    runtime
        .provide_requested_json(
            &requests[0],
            Arc::<str>::from(r#"{"color":{"$type":"fontFamily","$value":"red"}}"#),
        )
        .unwrap();
    assert!(matches!(
        runtime.resolve(&[("theme", "dark")]).unwrap(),
        Resolution::Missing(_)
    ));
    runtime
        .provide_requested_json(
            &requests[1],
            Arc::<str>::from(r#"{"color":{"$type":"fontFamily","$value":"blue"}}"#),
        )
        .unwrap();

    assert!(matches!(
        runtime.resolve(&[("theme", "dark")]).unwrap(),
        Resolution::Ready(_)
    ));
}

#[test]
fn lazy_catalog_batch_mutation_is_atomic_and_advances_one_revision() {
    let mut runtime = DtcgRuntime::from_json("root.json", "{}").unwrap();
    let initial_revision = runtime.catalog_revision();

    assert!(
        runtime
            .provide_json_batch([
                ("one.json", Arc::<str>::from("{}")),
                ("two.json", Arc::<str>::from("not json")),
            ])
            .is_err()
    );
    assert_eq!(runtime.catalog_revision(), initial_revision);
    assert!(!runtime.contains_document("one.json").unwrap());

    runtime
        .provide_json_batch([
            ("one.json", Arc::<str>::from("{}")),
            ("two.json", Arc::<str>::from("{}")),
        ])
        .unwrap();
    assert_eq!(runtime.catalog_revision(), initial_revision + 1);
    runtime.replace_json("one.json", "{}").unwrap();
    assert_eq!(runtime.catalog_revision(), initial_revision + 2);
    runtime.remove_document("two.json").unwrap();
    assert_eq!(runtime.catalog_revision(), initial_revision + 3);
}

#[test]
fn lazy_resolution_excludes_inactive_contexts_unless_complete_validation_is_requested() {
    let runtime = DtcgRuntime::from_json("workspace/root.json", ROOT).unwrap();

    let active = runtime.resolve(&[]).unwrap();
    let complete = runtime.validate_complete().unwrap();
    fn requested_ids<T>(resolution: Resolution<T>) -> Vec<String> {
        match resolution {
            Resolution::Missing(requests) => requests
                .into_iter()
                .map(|request| request.resource_id.to_string())
                .collect::<Vec<_>>(),
            Resolution::Ready(_) => panic!("documents have not been provided"),
        }
    }

    assert_eq!(
        requested_ids(active),
        ["workspace/tokens/base.json", "workspace/tokens/light.json"]
    );
    assert_eq!(
        requested_ids(complete),
        [
            "workspace/tokens/base.json",
            "workspace/tokens/dark.json",
            "workspace/tokens/light.json"
        ]
    );
}

#[test]
fn lazy_catalog_multi_level_cycles_and_limits_terminate_deterministically() {
    let root = r##"{
      "version":"2025.10",
      "resolutionOrder":[{"$ref":"#/sets/root"}],
      "sets":{"root":{"sources":[{"$ref":"nested.json"}]}}
    }"##;
    let nested = r##"{
      "version":"2025.10",
      "resolutionOrder":[{"$ref":"#/sets/leaf"}],
      "sets":{"leaf":{"sources":[{"$ref":"leaf.json"}]}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    let first = runtime.resolve(&[]).unwrap();
    let Resolution::Missing(first_requests) = first else {
        panic!("nested resolver must be missing")
    };
    runtime
        .provide_requested_json(&first_requests[0], Arc::<str>::from(nested))
        .unwrap();
    let Resolution::Missing(second_requests) = runtime.resolve(&[]).unwrap() else {
        panic!("newly provided resolver must reveal its leaf")
    };
    assert_eq!(second_requests[0].resource_id.as_str(), "leaf.json");

    let mut cycle = DtcgRuntime::from_json("root.json", root).unwrap();
    cycle.provide_json("nested.json", root).unwrap();
    assert!(cycle.resolve(&[]).is_err());

    let limited = DtcgRuntime::with_limits(
        RuntimeLimits::default()
            .with_max_documents(4)
            .with_max_reference_depth(0)
            .with_max_alias_depth(4),
    );
    let mut limited = limited;
    limited.provide_json("root.json", root).unwrap();
    limited.set_root("root.json").unwrap();
    assert!(limited.resolve(&[]).is_err());
}
