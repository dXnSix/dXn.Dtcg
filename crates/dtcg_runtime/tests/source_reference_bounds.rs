use std::time::{Duration, Instant};

use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution, RuntimeLimits, SourceReference};
use serde_json::{Value, json};

/// A root whose set holds `m` reference objects to `nested.json`, differing
/// only in `description`.
fn root(m: usize) -> Value {
    let sources: Vec<Value> = (0..m)
        .map(|i| json!({"$ref": "nested.json", "description": format!("copy {i}")}))
        .collect();
    json!({"version": "2025.10", "sets": {"base": {"sources": sources}},
        "resolutionOrder": [{"$ref": "#/sets/base"}]})
}

/// A nested resolver whose set holds `n` local references.
fn nested(n: usize) -> Value {
    let sources: Vec<Value> = (0..n)
        .map(|j| json!({"$ref": format!("#/$defs/t{j}")}))
        .collect();
    let defs: serde_json::Map<String, Value> = (0..n)
        .map(|j| {
            (
                format!("t{j}"),
                json!({format!("n{j}"): {"$type": "number", "$value": j}}),
            )
        })
        .collect();
    json!({"version": "2025.10", "$defs": defs, "sets": {"base": {"sources": sources}},
        "resolutionOrder": [{"$ref": "#/sets/base"}]})
}

fn runtime(limits: RuntimeLimits, m: usize, n: usize) -> DtcgRuntime {
    let mut runtime = DtcgRuntime::with_limits(limits);
    runtime
        .provide_json("root.json", root(m).to_string())
        .unwrap();
    runtime
        .provide_json("nested.json", nested(n).to_string())
        .unwrap();
    runtime.set_root("root.json").unwrap();
    runtime
}

fn sites(runtime: &DtcgRuntime) -> Vec<SourceReference> {
    match runtime.source_references().unwrap() {
        Resolution::Ready(sites) => sites,
        other => panic!("all documents are provided: {other:?}"),
    }
}

fn site_keys(sites: &[SourceReference]) -> Vec<(String, String)> {
    sites
        .iter()
        .map(|site| {
            (
                site.origin().resource().unwrap().as_str().to_owned(),
                site.origin().pointer().to_owned(),
            )
        })
        .collect()
}

#[test]
fn every_authored_site_is_listed_once() {
    let runtime = runtime(RuntimeLimits::default(), 4, 3);
    let sites = sites(&runtime);
    let keys = site_keys(&sites);
    let mut expected: Vec<(String, String)> = (0..4)
        .map(|i| ("root.json".into(), format!("/sets/base/sources/{i}/$ref")))
        .collect();
    expected.extend((0..3).map(|j| ("nested.json".into(), format!("/sets/base/sources/{j}/$ref"))));
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        keys.len(),
        "no site is listed twice: {keys:?}"
    );
    expected.sort();
    assert_eq!(sorted, expected);
}

#[test]
fn a_large_fan_out_fails_with_the_documents_limit_quickly() {
    let runtime = runtime(RuntimeLimits::default(), 400, 400);
    let started = Instant::now();
    let error = runtime.source_references().unwrap_err();
    let elapsed = started.elapsed();
    assert!(
        matches!(
            error,
            DtcgError::LimitExceeded {
                kind: "documents",
                limit: 256
            }
        ),
        "{error:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the walk stops at the limit, took {elapsed:?}"
    );
    // Discovery rejects the same document with the same limit.
    assert!(matches!(
        runtime.validate_complete(),
        Err(DtcgError::LimitExceeded {
            kind: "documents",
            ..
        })
    ));
}

#[test]
fn the_limit_setter_bounds_the_walk() {
    // The first reference object walks the nested resolver for free. Each of
    // the other three walks it again under its own `description` override:
    // seven repeated visits (the loaded target, the resolver, its set, its
    // resolution step and its three local references, whose token targets
    // lead nowhere) producing six steps, 13 each and 39 in total.
    let at_limit = runtime(RuntimeLimits::default().with_max_documents(39), 4, 3);
    assert_eq!(sites(&at_limit).len(), 7);
    let below = runtime(RuntimeLimits::default().with_max_documents(38), 4, 3);
    assert!(matches!(
        below.source_references(),
        Err(DtcgError::LimitExceeded {
            kind: "documents",
            limit: 38
        })
    ));
}

#[test]
fn overrides_still_decide_which_sites_are_walked() {
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default());
    runtime
        .provide_json(
            "root.json",
            json!({"version": "2025.10", "sets": {"base": {"sources": [
                {"$ref": "nested.json", "sets": {"base": {"sources": []}}}
            ]}}, "resolutionOrder": [{"$ref": "#/sets/base"}]})
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json("nested.json", nested(3).to_string())
        .unwrap();
    runtime.set_root("root.json").unwrap();
    let keys = site_keys(&sites(&runtime));
    // The replaced set's local references are not walked.
    assert!(
        !keys
            .iter()
            .any(|(resource, pointer)| resource == "nested.json"
                && pointer.starts_with("/sets/base/sources/")),
        "{keys:?}"
    );
}

/// A resolver whose one set holds `n` inline token documents.
fn inline_resolver(n: usize) -> Value {
    let sources: Vec<Value> = (0..n)
        .map(|j| json!({format!("n{j}"): {"$type": "number", "$value": j}}))
        .collect();
    json!({"version": "2025.10", "sets": {"base": {"sources": sources}},
        "resolutionOrder": [{"$ref": "#/sets/base"}]})
}

#[test]
fn inline_sources_are_walked_in_linear_time() {
    let runtime = DtcgRuntime::from_json("root.json", inline_resolver(20_000).to_string()).unwrap();
    let started = Instant::now();
    assert!(sites(&runtime).is_empty());
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "20,000 inline sources took {elapsed:?}"
    );
}

#[test]
fn repeated_references_without_overrides_are_walked_once() {
    // Plain references to the same nested resolver reach the same walk, so
    // they cost nothing however large the nested resolver is.
    let sources: Vec<Value> = (0..300).map(|_| json!({"$ref": "nested.json"})).collect();
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default());
    runtime
        .provide_json(
            "root.json",
            json!({"version": "2025.10", "sets": {"base": {"sources": sources}},
                "resolutionOrder": [{"$ref": "#/sets/base"}]})
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json("nested.json", inline_resolver(5_000).to_string())
        .unwrap();
    runtime.set_root("root.json").unwrap();
    let started = Instant::now();
    assert_eq!(sites(&runtime).len(), 300);
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
}

#[test]
fn overridden_references_to_a_large_inline_resolver_reach_the_limit_quickly() {
    let sources: Vec<Value> = (0..100)
        .map(|i| json!({"$ref": "nested.json", "description": format!("copy {i}")}))
        .collect();
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default());
    runtime
        .provide_json(
            "root.json",
            json!({"version": "2025.10", "sets": {"base": {"sources": sources}},
                "resolutionOrder": [{"$ref": "#/sets/base"}]})
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json("nested.json", inline_resolver(2_000).to_string())
        .unwrap();
    runtime.set_root("root.json").unwrap();
    let started = Instant::now();
    assert!(matches!(
        runtime.source_references(),
        Err(DtcgError::LimitExceeded {
            kind: "documents",
            limit: 256
        })
    ));
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
}

#[test]
fn a_large_dormant_set_is_listed_as_discovery_accepts_it() {
    let sources: Vec<Value> = (0..300)
        .map(|j| json!({"$ref": format!("#/$defs/t{j}")}))
        .collect();
    let mut document = nested(300);
    document["sets"]["dormant"] = json!({"sources": sources});
    document["sets"]["base"] = json!({"sources": [{"$ref": "#/$defs/t0"}]});
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    assert!(runtime.validate_complete().unwrap().is_ready());
    assert!(runtime.resolve(&[]).unwrap().is_ready());
    assert_eq!(sites(&runtime).len(), 301);
}

#[test]
fn many_overridden_references_to_one_large_token_document_are_cheap() {
    let big: serde_json::Map<String, Value> = (0..20_000)
        .map(|j| (format!("n{j}"), json!({"$type": "number", "$value": j})))
        .collect();
    let sources: Vec<Value> = (0..1_000)
        .map(|i| json!({"$ref": "big.json", "description": format!("copy {i}")}))
        .chain(
            (0..1_000).map(|i| json!({"$ref": "#/$defs/big", "description": format!("local {i}")})),
        )
        .collect();
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default());
    runtime
        .provide_json(
            "root.json",
            json!({"version": "2025.10", "$defs": {"big": big.clone()},
                "sets": {"dormant": {"sources": sources}, "base": {"sources": []}},
                "resolutionOrder": [{"$ref": "#/sets/base"}]})
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json("big.json", Value::Object(big).to_string())
        .unwrap();
    runtime.set_root("root.json").unwrap();
    let started = Instant::now();
    assert_eq!(sites(&runtime).len(), 2_000);
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
}
