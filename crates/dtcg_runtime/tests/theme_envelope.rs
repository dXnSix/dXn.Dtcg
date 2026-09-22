// Envelope, local-reference policy, modifier enumeration and selection
// normalization shared by RayX themes and the Token Editor (design D6).
use std::path::{Path, PathBuf};

use dtcg_runtime::{
    DtcgError, DtcgRuntime, LocalReferencePolicy, ModifierAxis, Resolution, ResolverEnvelope,
    ResourceId,
};

fn export_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rayx_components/rayx_components_core/assets/dtcg/rayx")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(export_dir().join(name)).expect("committed export file")
}

/// Loads the committed export the way a theme loader does: the envelope is the
/// root, every requested document is read relative to it until nothing is missing.
fn load_export() -> DtcgRuntime {
    let (mut runtime, envelope) =
        DtcgRuntime::from_envelope("rayx.tokeneditor", &read("rayx.tokeneditor")).unwrap();
    assert_eq!(envelope.name(), Some("rayx"));
    let mut loaded = 0;
    loop {
        match runtime.validate_complete().unwrap() {
            Resolution::Ready(()) => break,
            Resolution::Missing(requests) => {
                for request in requests {
                    LocalReferencePolicy::check(
                        &request.referenced_from,
                        &request.original_reference,
                    )
                    .unwrap();
                    runtime
                        .provide_requested_json(&request, read(request.resource_id.as_str()))
                        .unwrap();
                    loaded += 1;
                }
            }
        }
    }
    assert_eq!(loaded, 9, "the export references nine token documents");
    runtime
}

#[test]
fn envelope_exposes_the_resolver_and_rejects_malformed_envelopes() {
    let resource = ResourceId::parse("project.tokeneditor").unwrap();
    let envelope = ResolverEnvelope::parse(&resource, &read("rayx.tokeneditor")).unwrap();
    assert_eq!(envelope.name(), Some("rayx"));
    assert!(envelope.resolver_json().contains("\"resolutionOrder\""));

    let missing_data = r#"{"name":"x"}"#;
    assert!(matches!(
        ResolverEnvelope::parse(&resource, missing_data).unwrap_err(),
        DtcgError::InvalidEnvelope { ref message, .. } if message.contains("`data`")
    ));
    let data_not_object = r#"{"data":[]}"#;
    assert!(matches!(
        ResolverEnvelope::parse(&resource, data_not_object).unwrap_err(),
        DtcgError::InvalidEnvelope { .. }
    ));
    let wrong_version = r#"{"data":{"version":"2024.10","sets":{},"resolutionOrder":[]}}"#;
    assert!(matches!(
        ResolverEnvelope::parse(&resource, wrong_version).unwrap_err(),
        DtcgError::InvalidEnvelope { ref message, .. } if message.contains("2024.10")
    ));
    assert!(matches!(
        ResolverEnvelope::parse(&resource, "{").unwrap_err(),
        DtcgError::InvalidJson { .. }
    ));
}

#[test]
fn only_relative_local_references_pass_the_policy() {
    let resource = ResourceId::parse("project.tokeneditor").unwrap();
    for reference in [
        "tokens.json",
        "../shared/base.json",
        "a/b.json#/x",
        "#/sets/base",
    ] {
        LocalReferencePolicy::check(&resource, reference).unwrap();
    }
    for (reference, expected) in [
        ("", "empty"),
        ("/etc/tokens.json", "absolute"),
        ("C:\\tokens.json", "backslash"),
        ("https://example.com/tokens.json", "URI scheme"),
        ("file:tokens.json", "URI scheme"),
    ] {
        let error = LocalReferencePolicy::check(&resource, reference).unwrap_err();
        assert!(
            matches!(error, DtcgError::InvalidReference { ref message, reference: ref rejected, .. }
                if message.contains(expected) && rejected == reference),
            "{reference}: {error}"
        );
    }
    let resolver = serde_json::json!({
        "version": "2025.10",
        "sets": {"base": {"sources": [{"$ref": "base.json"}]}},
        "modifiers": {"theme": {"contexts": {"dark": [{"$ref": "/abs/dark.json"}]}}},
        "resolutionOrder": []
    });
    assert!(matches!(
        LocalReferencePolicy::check_resolver(&resource, &resolver).unwrap_err(),
        DtcgError::InvalidReference { ref reference, .. } if reference == "/abs/dark.json"
    ));
    let envelope = serde_json::json!({"data": resolver}).to_string();
    let Err(error) = DtcgRuntime::from_envelope("project.tokeneditor", &envelope) else {
        panic!("an envelope with an absolute reference must not open");
    };
    assert!(matches!(error, DtcgError::InvalidReference { .. }));
}

#[test]
fn modifiers_are_enumerated_in_resolution_order_with_defaults() {
    let runtime = load_export();
    let axes = runtime.modifiers().unwrap().ready().unwrap();
    let axis = |name: &str, contexts: &[&str], default: &str| ModifierAxis {
        name: name.into(),
        contexts: contexts.iter().map(|context| (*context).into()).collect(),
        default: Some(default.into()),
    };
    assert_eq!(
        axes,
        vec![
            axis("colorMode", &["light", "dark"], "light"),
            axis("accent", &["cyan", "sky", "blue"], "cyan"),
            axis("platform", &["desktop", "mobile"], "desktop"),
        ]
    );

    let inline = r##"{
      "version":"2025.10",
      "modifiers":{"density":{"contexts":{"compact":[],"cozy":[]}}},
      "resolutionOrder":[
        {"type":"modifier","name":"theme","contexts":{"dark":[],"light":[]},"default":"dark"}
      ]
    }"##;
    let runtime = DtcgRuntime::from_json("inline.resolver.json", inline).unwrap();
    let axes = runtime.modifiers().unwrap().ready().unwrap();
    assert_eq!(
        axes.iter()
            .map(|axis| axis.name.as_str())
            .collect::<Vec<_>>(),
        ["theme", "density"]
    );
    assert_eq!(axes[1].default, None);

    let plain =
        DtcgRuntime::from_json("tokens.json", r#"{"a":{"$type":"number","$value":1}}"#).unwrap();
    assert_eq!(plain.modifiers().unwrap().ready().unwrap(), Vec::new());

    let mut absent = DtcgRuntime::default();
    absent.set_root("missing.tokeneditor").unwrap();
    assert!(
        matches!(absent.modifiers().unwrap(), Resolution::Missing(requests) if requests.len() == 1)
    );
}

#[test]
fn selections_normalize_to_ordered_defaults_and_name_the_allowed_contexts() {
    let runtime = load_export();
    let normalize = |selection: &[(&str, &str)]| runtime.normalize_selection(selection);
    let owned = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(name, context)| ((*name).to_owned(), (*context).to_owned()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        normalize(&[]).unwrap().ready().unwrap(),
        owned(&[
            ("colorMode", "light"),
            ("accent", "cyan"),
            ("platform", "desktop")
        ])
    );
    let explicit = normalize(&[("platform", "mobile"), ("accent", "blue")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(
        explicit,
        owned(&[
            ("colorMode", "light"),
            ("accent", "blue"),
            ("platform", "mobile")
        ])
    );
    assert_eq!(
        explicit,
        normalize(&[
            ("accent", "blue"),
            ("platform", "mobile"),
            ("colorMode", "light")
        ])
        .unwrap()
        .ready()
        .unwrap(),
        "order and explicit defaults do not change the normalized selection"
    );

    let error = normalize(&[("density", "compact")]).unwrap_err();
    assert!(matches!(
        error,
        DtcgError::UnknownModifier { ref modifier, ref known }
            if modifier == "density" && known == &["colorMode", "accent", "platform"]
    ));
    assert!(error.to_string().contains("colorMode, accent, platform"));

    let error = normalize(&[("accent", "green")]).unwrap_err();
    assert!(matches!(
        error,
        DtcgError::UnknownContext { ref modifier, ref context, ref allowed, .. }
            if modifier == "accent" && context == "green" && allowed == &["cyan", "sky", "blue"]
    ));
    assert!(error.to_string().contains("cyan, sky, blue"));

    assert!(matches!(
        normalize(&[("accent", "sky"), ("accent", "blue")]).unwrap_err(),
        DtcgError::UnknownContext { ref message, .. } if message.contains("twice")
    ));

    let no_default = DtcgRuntime::from_json(
        "inline.resolver.json",
        r##"{"version":"2025.10","modifiers":{"density":{"contexts":{"compact":[],"cozy":[]}}},"resolutionOrder":[{"$ref":"#/modifiers/density"}]}"##,
    )
    .unwrap();
    assert!(matches!(
        no_default.normalize_selection(&[]).unwrap_err(),
        DtcgError::UnknownContext { ref modifier, ref allowed, .. }
            if modifier == "density" && allowed == &["compact", "cozy"]
    ));
    assert_eq!(
        no_default
            .normalize_selection(&[("density", "cozy")])
            .unwrap()
            .ready()
            .unwrap(),
        owned(&[("density", "cozy")])
    );
}

#[test]
fn the_committed_export_resolves_from_its_envelope_for_every_selection() {
    let runtime = load_export();
    let axes = runtime.modifiers().unwrap().ready().unwrap();
    let mut fingerprints = std::collections::BTreeSet::new();
    let mut token_counts = std::collections::BTreeSet::new();
    for color_mode in &axes[0].contexts {
        for accent in &axes[1].contexts {
            for platform in &axes[2].contexts {
                let selection = [
                    ("colorMode", color_mode.as_str()),
                    ("accent", accent.as_str()),
                    ("platform", platform.as_str()),
                ];
                let tokens = runtime.resolve(&selection).unwrap().ready().unwrap();
                token_counts.insert(tokens.len());
                fingerprints.insert(tokens.fingerprint().to_owned());
            }
        }
    }
    assert_eq!(
        token_counts.len(),
        1,
        "every selection yields the same token set"
    );
    assert_eq!(
        fingerprints.len(),
        12,
        "every selection resolves to distinct values"
    );
}
