use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution, ResourceId, RuntimeLimits};
use serde_json::{Value, json};

fn runtime(data: Value) -> DtcgRuntime {
    DtcgRuntime::from_json("workspace/root.json", data.to_string()).unwrap()
}

fn resolver(sources: Vec<Value>) -> Value {
    json!({"version":"2025.10", "sets":{"base":{"sources":sources}},
        "resolutionOrder":[{"$ref":"#/sets/base"}]})
}

fn number(value: i32) -> Value {
    json!({"number":{"$type":"number","$value":value}})
}

#[test]
fn inspection_preserves_local_set_occurrences_empty_groups_and_override_owners() {
    let mut data = resolver(vec![json!({"$ref":"#/sets/template"})]);
    data["sets"]["template"] = json!({"sources":[
        {"$ref":"#/$defs/palette", "number":{"$type":"number","$value":2}},
        {"$ref":"#/$defs/palette"}
    ]});
    data["$defs"] = json!({"palette":{"number":{"$type":"number","$value":1},"empty":{}}});
    let runtime = runtime(data);
    let sources = runtime
        .inspect_source(&json!({"$ref":"#/sets/template"}), &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(
        sources[0].origin().resource().unwrap().as_str(),
        "workspace/root.json"
    );
    assert_eq!(sources[0].origin().pointer(), "/$defs/palette");
    assert_eq!(
        sources[0].member_origins()["number"].pointer(),
        "/sets/template/sources/0"
    );
    assert_eq!(sources[0].value()["empty"], json!({}));
    assert_eq!(
        runtime
            .preview_occurrence(&sources[0], &[])
            .unwrap()
            .ready()
            .unwrap()
            .get("number")
            .unwrap()
            .value,
        2
    );
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("number")
            .unwrap()
            .value,
        1
    );

    let supplied = json!({"$ref":"#/sets/template", "sources":[number(9)]});
    let overridden = runtime
        .inspect_source(&supplied, &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(overridden[0].origin().resource(), None);
    assert_eq!(overridden[0].origin().pointer(), "/sources/0");
}

#[test]
fn repeated_resolver_documents_participate_at_every_authored_position() {
    let mut runtime = runtime(resolver(vec![
        json!({"$ref":"nested.json"}),
        number(8),
        json!({"$ref":"nested.json"}),
    ]));
    runtime
        .provide_json(
            "workspace/nested.json",
            resolver(vec![number(3)]).to_string(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("number")
            .unwrap()
            .value,
        3
    );
    let supplied = resolver(vec![
        json!({"$ref":"nested.json"}),
        number(8),
        json!({"$ref":"nested.json"}),
    ]);
    let sources = runtime
        .inspect_source(&supplied, &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(sources.len(), 3);
    assert_eq!(sources[0].origin(), sources[2].origin());
    assert_eq!(sources[1].value()["number"]["$value"], 8);
}

#[test]
fn nested_inline_resolvers_and_escaped_locations_are_not_document_cycles() {
    let inner = json!({"version":"2025.10", "sets":{"a/b~c":{"sources":[number(7)]}},
        "resolutionOrder":[{"$ref":"#/sets/a~1b~0c"}]});
    let runtime = runtime(resolver(vec![inner.clone()]));
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("number")
            .unwrap()
            .value,
        7
    );
    let sources = runtime
        .inspect_source(&inner, &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(sources[0].origin().pointer(), "/sets/a~1b~0c/sources/0");
    assert!(sources[0].origin().resource().is_none());

    let mut cyclic = runtime;
    cyclic
        .provide_json(
            "workspace/cycle.json",
            resolver(vec![json!({"$ref":"cycle.json"})]).to_string(),
        )
        .unwrap();
    assert!(matches!(
        cyclic.inspect_source(&json!({"$ref":"cycle.json"}), &[]),
        Err(DtcgError::DocumentCycle(_))
    ));
}

#[test]
fn source_discovery_acquires_dormant_contexts_without_resolving_values() {
    let mut runtime = runtime(resolver(vec![number(1)]));
    let source = json!({"version":"2025.10", "modifiers":{"mode":{"default":"light", "contexts":{
        "light":[number(2)], "dark":[{"$ref":"modes/dark.json"}]
    }}}, "resolutionOrder":[{"$ref":"#/modifiers/mode"}]});
    let Resolution::Missing(requests) = runtime.validate_source(&source).unwrap() else {
        panic!("dormant source must be requested")
    };
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].resource_id.as_str(),
        "workspace/modes/dark.json"
    );
    assert_eq!(requests[0].referenced_from.as_str(), "workspace/root.json");
    runtime
        .provide_requested_json(
            &requests[0],
            r#"{"number":{"$type":"number","$value":"{missing}"}}"#,
        )
        .unwrap();
    assert!(matches!(
        runtime.validate_source(&source).unwrap(),
        Resolution::Ready(())
    ));
    assert_eq!(
        runtime
            .preview_source(&source, &[])
            .unwrap()
            .ready()
            .unwrap()
            .get("number")
            .unwrap()
            .value,
        2
    );
    // Inspection is independent of token-value validity; final preview validates it.
    assert!(
        runtime
            .inspect_source(&json!({"$ref":"modes/dark.json"}), &[])
            .unwrap()
            .ready()
            .is_some()
    );
    assert!(
        runtime
            .preview_source(&json!({"$ref":"modes/dark.json"}), &[])
            .is_err()
    );
}

#[test]
fn external_occurrence_preview_keeps_fragment_metadata_and_catalog_unchanged() {
    let mut runtime = runtime(resolver(vec![
        json!({"$ref":"first.json#/palette"}),
        json!({"$ref":"second.json#/palette"}),
    ]));
    for (file, number) in [("first.json", 1), ("second.json", 2)] {
        runtime
            .provide_json(
                format!("workspace/{file}"),
                json!({"palette":{
                    "$extensions":{"example":{"number":number}},
                    "alias":{"$type":"number","$value":{"$ref":"#/$extensions/example/number"}}
                }})
                .to_string(),
            )
            .unwrap();
    }
    let revision = runtime.catalog_revision();
    let fingerprint = runtime
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
        .fingerprint()
        .to_owned();
    let sources = runtime
        .inspect_source(&json!({"$ref":"first.json#/palette"}), &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(sources[0].origin().pointer(), "/palette");
    assert_eq!(
        runtime
            .preview_occurrence(&sources[0], &[])
            .unwrap()
            .ready()
            .unwrap()
            .get("alias")
            .unwrap()
            .value,
        1
    );
    assert_eq!(runtime.catalog_revision(), revision);
    assert_eq!(
        runtime.resolve(&[]).unwrap().ready().unwrap().fingerprint(),
        fingerprint
    );
}

#[test]
fn source_and_preview_limits_are_shared_with_resolution() {
    let mut runtime = DtcgRuntime::with_limits(
        RuntimeLimits::default()
            .with_max_documents(2)
            .with_max_reference_depth(2)
            .with_max_alias_depth(2),
    );
    runtime
        .provide_json(
            "root.json",
            resolver(vec![number(1), number(2)]).to_string(),
        )
        .unwrap();
    runtime.set_root("root.json").unwrap();
    assert!(runtime.resolve(&[]).unwrap().ready().is_some());
    assert!(matches!(
        runtime.preview_source(&number(3), &[]),
        Err(DtcgError::LimitExceeded {
            kind: "documents",
            ..
        })
    ));
    assert!(matches!(
        runtime.inspect_source(&resolver(vec![number(1), number(2), number(3)]), &[]),
        Err(DtcgError::LimitExceeded {
            kind: "documents",
            ..
        })
    ));
    let mut deep = number(1);
    for _ in 0..4 {
        deep = resolver(vec![deep]);
    }
    // Every expansion step counts against `max_documents`, so the depth probe
    // needs room for the nested resolvers it expands.
    let mut nested = DtcgRuntime::with_limits(
        RuntimeLimits::default()
            .with_max_documents(64)
            .with_max_reference_depth(2),
    );
    nested
        .provide_json("root.json", resolver(vec![number(1)]).to_string())
        .unwrap();
    nested.set_root("root.json").unwrap();
    assert!(matches!(
        nested.inspect_source(&deep, &[]),
        Err(DtcgError::LimitExceeded {
            kind: "reference depth",
            ..
        })
    ));

    let mut alias_runtime =
        DtcgRuntime::with_limits(RuntimeLimits::default().with_max_alias_depth(2));
    alias_runtime.provide_json("root.json", resolver(vec![json!({
        "a":{"$type":"number","$value":1}, "b":{"$type":"number","$value":"{a}"}, "c":{"$type":"number","$value":"{b}"}
    })]).to_string()).unwrap();
    alias_runtime.set_root("root.json").unwrap();
    assert!(alias_runtime.resolve(&[]).unwrap().ready().is_some());
    let reverse: Value = serde_json::from_str(r#"{"c":{"$type":"number","$value":"{b}"},"b":{"$type":"number","$value":"{a}"},"a":{"$type":"number","$value":1}}"#).unwrap();
    // Alias depth counts reference hops, whatever the authored order.
    assert!(
        alias_runtime
            .preview_source(&reverse, &[])
            .unwrap()
            .ready()
            .is_some()
    );
    let mut shallow = DtcgRuntime::with_limits(RuntimeLimits::default().with_max_alias_depth(1));
    shallow
        .provide_json("root.json", resolver(vec![json!({})]).to_string())
        .unwrap();
    shallow.set_root("root.json").unwrap();
    assert!(matches!(
        shallow.preview_source(&reverse, &[]),
        Err(DtcgError::LimitExceeded {
            kind: "alias depth",
            ..
        })
    ));
}

#[test]
fn reference_sites_include_dormant_and_overridden_sources_but_exclude_extensions_and_aliases() {
    let mut data = resolver(vec![json!({"$ref":"nested/wrapper.json#/embedded"})]);
    data["sets"]["dormant"] = json!({"sources":[{"$ref":"unused.json"}]});
    data["sets"]["base"]["$extensions"] = json!({"example":{"$ref":"do-not-load.json"}});
    let mut runtime = runtime(data);
    runtime
        .provide_json(
            "workspace/nested/wrapper.json",
            json!({"embedded":resolver(vec![json!({"$ref":"leaf.json"})]),
        "unknown":{"$ref":"do-not-load.json"}})
            .to_string(),
        )
        .unwrap();
    let Resolution::Missing(requests) = runtime.source_references().unwrap() else {
        panic!("source sites need retained bytes")
    };
    assert_eq!(
        requests
            .iter()
            .map(|request| request.resource_id.as_str())
            .collect::<Vec<_>>(),
        vec!["workspace/nested/leaf.json", "workspace/unused.json"]
    );
    for request in requests {
        runtime
            .provide_requested_json(
                &request,
                json!({"n":{"$type":"number","$value":{"$ref":"#/other/$value"}}}).to_string(),
            )
            .unwrap();
    }
    let sites = runtime.source_references().unwrap().ready().unwrap();
    assert_eq!(sites.len(), 3);
    let nested = sites
        .iter()
        .find(|site| site.reference().original() == "leaf.json")
        .unwrap();
    assert_eq!(
        nested.origin().resource().unwrap().as_str(),
        "workspace/nested/wrapper.json"
    );
    assert_eq!(
        nested.origin().pointer(),
        "/embedded/sets/base/sources/0/$ref"
    );
    assert_eq!(
        nested.reference().resource_id().as_str(),
        "workspace/nested/leaf.json"
    );
}

#[test]
fn definition_accessors_validate_standard_shapes_without_editor_default_policy() {
    let id = ResourceId::parse("definition.json").unwrap();
    assert!(
        dxn_dtcg::set_sources(&id, &json!({"sources":[]}))
            .unwrap()
            .is_empty()
    );
    assert!(dxn_dtcg::set_sources(&id, &json!({"sources":{}})).is_err());
    assert!(dxn_dtcg::modifier_contexts(&id, &json!({"contexts":{"dark":[], "light":[]}})).is_ok());
    // Resolver §4.2: a modifier needs at least two contexts.
    assert!(dxn_dtcg::modifier_contexts(&id, &json!({"contexts":{"dark":[]}})).is_err());
    assert!(
        dxn_dtcg::modifier_contexts(
            &id,
            &json!({"contexts":{"dark":[], "light":[]}, "default":"missing"})
        )
        .is_err()
    );
    assert!(dxn_dtcg::modifier_contexts(&id, &json!({"contexts":{}})).is_err());
}
