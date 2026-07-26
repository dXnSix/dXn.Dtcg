use std::time::Instant;

use dtcg_runtime::{DtcgRuntime, Resolution};

#[test]
fn records_cold_repeat_context_switch_and_invalidation_baselines() {
    let root = r##"{
      "resolutionOrder":[{"$ref":"#/sets/base"},{"$ref":"#/modifiers/theme"}],
      "sets":{"base":{"sources":["base.json"]}},
      "modifiers":{"theme":{"default":"light","contexts":{"light":["light.json"],"dark":["dark.json"]}}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    runtime
        .provide_json("base.json", r#"{"color":{"$value":"red"}}"#)
        .unwrap();
    runtime
        .provide_json("light.json", r#"{"color":{"$value":"white"}}"#)
        .unwrap();
    runtime
        .provide_json("dark.json", r#"{"color":{"$value":"black"}}"#)
        .unwrap();

    let cold_start = Instant::now();
    assert!(matches!(
        runtime.resolve(&[]).unwrap(),
        Resolution::Ready(_)
    ));
    let cold = cold_start.elapsed();
    let repeat_start = Instant::now();
    assert!(matches!(
        runtime.resolve(&[]).unwrap(),
        Resolution::Ready(_)
    ));
    let repeat = repeat_start.elapsed();
    let switch_start = Instant::now();
    assert!(matches!(
        runtime.resolve(&[("theme", "dark")]).unwrap(),
        Resolution::Ready(_)
    ));
    let switch = switch_start.elapsed();
    runtime
        .replace_json("dark.json", r#"{"color":{"$value":"gray"}}"#)
        .unwrap();
    let invalidation_start = Instant::now();
    assert!(matches!(
        runtime.resolve(&[("theme", "dark")]).unwrap(),
        Resolution::Ready(_)
    ));
    let invalidation = invalidation_start.elapsed();

    eprintln!("cold={cold:?} repeat={repeat:?} switch={switch:?} invalidation={invalidation:?}");
}
