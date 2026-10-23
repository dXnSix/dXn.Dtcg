use std::path::Path;

use dxn_dtcg::{
    DtcgError, DtcgRuntime, FromDtcgValue, ResolvedTokens, RuntimeOptions, TypeRegistry,
    ValueExtensions,
    values::{
        Blur, BlurKind, Border, Color, ColorSpace, Component, CubicBezier, Dimension,
        DimensionUnit, Duration, DurationUnit, FontFamily, FontWeight, Gradient, LineCap,
        LineHeight, LineStyle, Shadow, StrokeStyle, Transition, TypedValue, Typography,
    },
};
use serde_json::{Value, json};

fn resolve(document: Value) -> ResolvedTokens {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

fn typed(tokens: &ResolvedTokens, path: &str) -> TypedValue {
    tokens
        .get(path)
        .unwrap()
        .typed(&ValueExtensions::default())
        .unwrap()
}

fn srgb(components: [f64; 3]) -> Value {
    json!({"colorSpace": "srgb", "components": components})
}

fn px(value: f64) -> Value {
    json!({"value": value, "unit": "px"})
}

#[test]
fn every_type_of_every_positive_corpus_fixture_converts() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dtcg-test-suite/2025.10/format");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    let mut types = std::collections::BTreeSet::new();
    for test in manifest["tests"].as_array().unwrap() {
        if test["type"] != "PositiveEvaluationTest" {
            continue;
        }
        let input = root.join(test["input"].as_str().unwrap());
        let tokens =
            DtcgRuntime::from_json("fixture.json", std::fs::read_to_string(&input).unwrap())
                .unwrap()
                .resolve(&[])
                .unwrap()
                .ready()
                .unwrap();
        for token in tokens.iter() {
            let token_type = token.token_type.as_deref().unwrap();
            let value = token
                .typed(&ValueExtensions::default())
                .unwrap_or_else(|error| panic!("{} in {}: {error}", token.path, input.display()));
            let matches = matches!(
                (token_type, &value),
                ("color", TypedValue::Color(_))
                    | ("dimension", TypedValue::Dimension(_))
                    | ("fontFamily", TypedValue::FontFamily(_))
                    | ("fontWeight", TypedValue::FontWeight(_))
                    | ("duration", TypedValue::Duration(_))
                    | ("cubicBezier", TypedValue::CubicBezier(_))
                    | ("number", TypedValue::Number(_))
                    | ("strokeStyle", TypedValue::StrokeStyle(_))
                    | ("border", TypedValue::Border(_))
                    | ("transition", TypedValue::Transition(_))
                    | ("shadow", TypedValue::Shadow(_))
                    | ("gradient", TypedValue::Gradient(_))
                    | ("typography", TypedValue::Typography(_))
            );
            assert!(
                matches,
                "{} is {token_type} but converted to {value:?}",
                token.path
            );
            types.insert(token_type.to_owned());
        }
    }
    assert_eq!(types.len(), 13, "{types:?}");
}

#[test]
fn colors_default_their_alpha_and_keep_none_components_and_hex() {
    let tokens = resolve(json!({
        "a": {"$type": "color", "$value": srgb([1.0, 0.5, 0.0])},
        "b": {"$type": "color", "$value": {"colorSpace": "oklch", "components": [0.5, 0.1, "none"], "alpha": 0.25, "hex": "#AbCdEf"}}
    }));
    assert_eq!(
        typed(&tokens, "a"),
        TypedValue::Color(Color {
            color_space: ColorSpace::Srgb,
            components: [
                Component::Number(1.0),
                Component::Number(0.5),
                Component::Number(0.0)
            ],
            alpha: 1.0,
            hex: None,
        })
    );
    let TypedValue::Color(b) = typed(&tokens, "b") else {
        panic!()
    };
    assert_eq!(b.color_space, ColorSpace::Oklch);
    assert_eq!(b.color_space.name(), "oklch");
    assert_eq!(b.components[2], Component::None);
    assert_eq!(b.alpha, 0.25);
    assert_eq!(b.hex.as_deref(), Some("#AbCdEf"));
}

#[test]
fn gradient_positions_are_clamped_and_values_keep_the_authored_numbers() {
    let tokens = resolve(json!({"g": {"$type": "gradient", "$value": [
        {"color": srgb([0.0, 0.0, 0.0]), "position": -99},
        {"color": srgb([1.0, 1.0, 1.0]), "position": 0.5},
        {"color": srgb([1.0, 1.0, 1.0]), "position": 42}
    ]}}));
    let TypedValue::Gradient(Gradient(stops)) = typed(&tokens, "g") else {
        panic!()
    };
    assert_eq!(
        stops.iter().map(|stop| stop.position).collect::<Vec<_>>(),
        [0.0, 0.5, 1.0]
    );
    assert_eq!(tokens.get("g").unwrap().value[2]["position"], json!(42));
}

#[test]
fn font_weight_keywords_map_to_numbers() {
    for (keyword, weight) in [
        ("thin", 100.0),
        ("hairline", 100.0),
        ("extra-light", 200.0),
        ("ultra-light", 200.0),
        ("light", 300.0),
        ("normal", 400.0),
        ("regular", 400.0),
        ("book", 400.0),
        ("medium", 500.0),
        ("semi-bold", 600.0),
        ("demi-bold", 600.0),
        ("bold", 700.0),
        ("extra-bold", 800.0),
        ("ultra-bold", 800.0),
        ("black", 900.0),
        ("heavy", 900.0),
        ("extra-black", 950.0),
        ("ultra-black", 950.0),
    ] {
        let tokens = resolve(json!({"w": {"$type": "fontWeight", "$value": keyword}}));
        assert_eq!(
            typed(&tokens, "w"),
            TypedValue::FontWeight(FontWeight(weight)),
            "{keyword}"
        );
    }
    let tokens = resolve(json!({"w": {"$type": "fontWeight", "$value": 450}}));
    assert_eq!(
        typed(&tokens, "w"),
        TypedValue::FontWeight(FontWeight(450.0))
    );
}

#[test]
fn shadows_become_layers_with_inset_defaulting_to_false() {
    let layer = json!({"color": srgb([0.0, 0.0, 0.0]), "offsetX": px(0.0), "offsetY": px(1.0), "blur": px(2.0), "spread": px(0.0)});
    let mut inset = layer.clone();
    inset["inset"] = json!(true);
    let tokens = resolve(json!({
        "single": {"$type": "shadow", "$value": layer},
        "layers": {"$type": "shadow", "$value": [layer, inset]}
    }));
    let TypedValue::Shadow(Shadow(single)) = typed(&tokens, "single") else {
        panic!()
    };
    assert_eq!(single.len(), 1);
    assert!(!single[0].inset);
    assert_eq!(
        single[0].offset_y,
        Dimension {
            value: 1.0,
            unit: DimensionUnit::Px
        }
    );
    let TypedValue::Shadow(Shadow(layers)) = typed(&tokens, "layers") else {
        panic!()
    };
    assert_eq!(
        layers.iter().map(|layer| layer.inset).collect::<Vec<_>>(),
        [false, true]
    );
}

#[test]
fn composite_and_unit_types_convert() {
    let tokens = resolve(json!({
        "d": {"$type": "dimension", "$value": {"value": 1.5, "unit": "rem"}},
        "t": {"$type": "duration", "$value": {"value": 0.2, "unit": "s"}},
        "e": {"$type": "cubicBezier", "$value": [0.4, 0, 0.2, 1]},
        "f": {"$type": "fontFamily", "$value": ["Inter", "sans-serif"]},
        "n": {"$type": "number", "$value": 3},
        "s": {"$type": "strokeStyle", "$value": "dotted"},
        "dash": {"$type": "strokeStyle", "$value": {"dashArray": [px(2.0), px(4.0)], "lineCap": "round"}},
        "b": {"$type": "border", "$value": {"color": srgb([0.0, 0.0, 0.0]), "width": px(1.0), "style": "solid"}},
        "tr": {"$type": "transition", "$value": {"duration": {"value": 100, "unit": "ms"}, "delay": {"value": 0, "unit": "ms"}, "timingFunction": [0, 0, 1, 1]}},
        "ty": {"$type": "typography", "$value": {"fontFamily": "Inter", "fontSize": px(16.0), "fontWeight": "bold", "letterSpacing": px(0.0), "lineHeight": 1.5}}
    }));
    let TypedValue::Dimension(dimension) = typed(&tokens, "d") else {
        panic!()
    };
    assert_eq!(dimension.to_px(16.0), 24.0);
    assert_eq!(
        Dimension {
            value: 3.0,
            unit: DimensionUnit::Px
        }
        .to_px(16.0),
        3.0
    );
    assert_eq!(
        typed(&tokens, "t"),
        TypedValue::Duration(Duration {
            value: 0.2,
            unit: DurationUnit::S
        })
    );
    assert_eq!(
        typed(&tokens, "e"),
        TypedValue::CubicBezier(CubicBezier([0.4, 0.0, 0.2, 1.0]))
    );
    assert_eq!(
        typed(&tokens, "f"),
        TypedValue::FontFamily(FontFamily(vec!["Inter".into(), "sans-serif".into()]))
    );
    assert_eq!(typed(&tokens, "n"), TypedValue::Number(3.0));
    assert_eq!(
        typed(&tokens, "s"),
        TypedValue::StrokeStyle(StrokeStyle::Style(LineStyle::Dotted))
    );
    let TypedValue::StrokeStyle(StrokeStyle::Dashes {
        dash_array,
        line_cap,
    }) = typed(&tokens, "dash")
    else {
        panic!()
    };
    assert_eq!((dash_array.len(), line_cap), (2, LineCap::Round));
    let TypedValue::Border(Border { width, style, .. }) = typed(&tokens, "b") else {
        panic!()
    };
    assert_eq!(
        (width.value, style),
        (1.0, StrokeStyle::Style(LineStyle::Solid))
    );
    let TypedValue::Transition(Transition { duration, .. }) = typed(&tokens, "tr") else {
        panic!()
    };
    assert_eq!(duration.to_millis(), 100.0);
    let TypedValue::Typography(Typography {
        font_weight,
        line_height,
        ..
    }) = typed(&tokens, "ty")
    else {
        panic!()
    };
    assert_eq!(font_weight, FontWeight(700.0));
    assert_eq!(line_height, LineHeight::Multiplier(1.5));
}

#[test]
fn the_rayx_extension_types_need_their_flags() {
    let document = json!({
        "blur": {"$type": "dev.rayx.blur", "$value": {"kind": "background", "radius": px(8.0)}},
        "heading": {"$type": "typography", "$value": {"fontFamily": "Inter", "fontSize": px(16.0), "fontWeight": 400, "letterSpacing": px(0.0), "lineHeight": px(24.0)}}
    });
    let tokens = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_extensions(ValueExtensions::rayx()),
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();
    let blur = tokens.get("blur").unwrap();
    assert_eq!(
        blur.typed(&ValueExtensions::rayx()).unwrap(),
        TypedValue::Blur(Blur {
            kind: BlurKind::Background,
            radius: Dimension {
                value: 8.0,
                unit: DimensionUnit::Px
            }
        })
    );
    assert!(matches!(
        blur.typed(&ValueExtensions::default()),
        Err(DtcgError::Projection { path, .. }) if path == "blur"
    ));
    let heading = tokens.get("heading").unwrap();
    let TypedValue::Typography(typography) = heading.typed(&ValueExtensions::rayx()).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        typography.line_height,
        LineHeight::Dimension(Dimension {
            value: 24.0,
            unit: DimensionUnit::Px
        })
    );
    assert!(heading.typed(&ValueExtensions::default()).is_err());
}

#[test]
fn model_types_project_through_from_dtcg_value() {
    let tokens = resolve(json!({
        "red": {"$type": "color", "$value": srgb([1.0, 0.0, 0.0])},
        "gap": {"$type": "dimension", "$value": px(8.0)},
        "ratio": {"$type": "number", "$value": 1.5},
        "huge": {"$type": "number", "$value": 1e300},
        "name": {"$type": "fontFamily", "$value": "Inter"}
    }));
    let registry = TypeRegistry;
    let red: Color = registry.convert(tokens.get("red").unwrap()).unwrap();
    assert_eq!(red.alpha, 1.0);
    let gap = Dimension::from_dtcg(tokens.get("gap").unwrap()).unwrap();
    assert_eq!(gap.to_px(16.0), 8.0);
    assert_eq!(
        LineHeight::from_dtcg(tokens.get("ratio").unwrap()).unwrap(),
        LineHeight::Multiplier(1.5)
    );
    assert_eq!(
        LineHeight::from_dtcg(tokens.get("gap").unwrap()).unwrap(),
        LineHeight::Dimension(gap)
    );
    assert!(matches!(
        TypedValue::from_dtcg(tokens.get("gap").unwrap()).unwrap(),
        TypedValue::Dimension(_)
    ));

    let mismatch = Color::from_dtcg(tokens.get("gap").unwrap()).unwrap_err();
    assert!(
        matches!(&mismatch, DtcgError::Projection { path, message } if path == "gap" && message == "expected a color value"),
        "{mismatch:?}"
    );
    assert!(matches!(
        Dimension::from_dtcg(tokens.get("name").unwrap()),
        Err(DtcgError::Projection { path, .. }) if path == "name"
    ));

    assert_eq!(f32::from_dtcg(tokens.get("ratio").unwrap()).unwrap(), 1.5);
    let overflow = f32::from_dtcg(tokens.get("huge").unwrap()).unwrap_err();
    assert!(
        matches!(&overflow, DtcgError::Projection { path, message }
            if path == "huge" && message == "expected numeric value within f32 range"),
        "{overflow:?}"
    );
    assert_eq!(f64::from_dtcg(tokens.get("huge").unwrap()).unwrap(), 1e300);
}
