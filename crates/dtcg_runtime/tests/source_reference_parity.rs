use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution, RuntimeLimits};
use serde_json::{Value, json};

/// A root `brand` modifier with `m` contexts, each reaching `brand.json` with
/// its own override: a different `description`, or its own `sets`.
fn brands(m: usize, override_sets: bool) -> DtcgRuntime {
    brands_with(RuntimeLimits::default(), m, override_sets)
}

fn brands_with(limits: RuntimeLimits, m: usize, override_sets: bool) -> DtcgRuntime {
    let local = |name: &str| json!({"$ref": format!("#/$defs/{name}")});
    let sets = |brand: usize| {
        json!({
            "a": {"sources": [local("t0"), local(&format!("t{}", brand % 4))]},
            "b": {"sources": [local("t1"), {"inline": {"$type": "number", "$value": brand}}]},
            "c": {"sources": [local("t3"), local("t0")]}
        })
    };
    let contexts: serde_json::Map<String, Value> = (0..m)
        .map(|brand| {
            let mut reference = json!({"$ref": "brand.json"});
            if override_sets {
                reference["sets"] = sets(brand);
            } else {
                reference["description"] = json!(format!("brand {brand}"));
            }
            (format!("b{brand}"), json!([reference]))
        })
        .collect();
    let defs: serde_json::Map<String, Value> = (0..4)
        .map(|j| {
            (
                format!("t{j}"),
                json!({format!("n{j}"): {"$type": "number", "$value": j}}),
            )
        })
        .collect();
    let mut runtime = DtcgRuntime::with_limits(limits);
    runtime
        .provide_json(
            "root.json",
            json!({"version": "2025.10",
                "modifiers": {"brand": {"contexts": contexts, "default": "b0"}},
                "resolutionOrder": [{"$ref": "#/modifiers/brand"}]})
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json(
            "brand.json",
            json!({"version": "2025.10", "$defs": defs, "sets": sets(0),
                "modifiers": {"mode": {"contexts": {"light": [local("t1")], "dark": [local("t2")]},
                    "default": "light"}},
                "resolutionOrder": [{"$ref": "#/sets/a"}, {"$ref": "#/sets/b"},
                    {"$ref": "#/sets/c"}, {"$ref": "#/modifiers/mode"}]})
            .to_string(),
        )
        .unwrap();
    runtime.set_root("root.json").unwrap();
    runtime
}

#[test]
fn every_document_validate_complete_accepts_is_listed() {
    for override_sets in [false, true] {
        let mut accepted = 0;
        for m in 2..=80 {
            let runtime = brands(m, override_sets);
            let complete = runtime.validate_complete();
            if !matches!(complete, Ok(Resolution::Ready(()))) {
                assert!(
                    matches!(
                        complete,
                        Err(DtcgError::LimitExceeded {
                            kind: "documents",
                            ..
                        })
                    ),
                    "{complete:?}"
                );
                continue;
            }
            accepted = m;
            let listed = runtime.source_references();
            assert!(
                matches!(listed, Ok(Resolution::Ready(_))),
                "{m} brands (override sets: {override_sets}) are complete but not listed: {listed:?}"
            );
        }
        assert!((10..80).contains(&accepted), "{accepted}");
    }
}

fn limit_exceeded(runtime: &DtcgRuntime, limit: usize) -> bool {
    matches!(
        runtime.source_references(),
        Err(DtcgError::LimitExceeded { kind: "documents", limit: exceeded }) if exceeded == limit
    )
}

#[test]
fn repeated_walks_count_steps_and_inline_sources_separately() {
    // Three brands: the first walks `brand.json` for free, and each of the
    // other two walks it again under its own `description`. A repeated walk
    // takes 8 steps (expanding the resolver and following 7 local references:
    // 2 in set `a`, 1 in `b`, 2 in `c`, 2 in the `mode` contexts) and
    // collects 1 inline source, so 16 steps and 2 sources in total.
    let at_limit = brands_with(RuntimeLimits::default().with_max_documents(16), 3, false);
    // Counted together, steps and sources (18) would exceed 16.
    assert!(matches!(
        at_limit.source_references(),
        Ok(Resolution::Ready(_))
    ));
    let below = brands_with(RuntimeLimits::default().with_max_documents(15), 3, false);
    assert!(limit_exceeded(&below, 15));
}
