//! The color API of `values::Color`: conversions between the 14 DTCG color
//! spaces with the CSS Color 4 formulas, and the operations built on them.

use dxn_dtcg::{
    ValueExtensions, validate_value_with,
    values::{Color, ColorSpace, Component},
};
use serde_json::json;

const SPACES: [ColorSpace; 14] = [
    ColorSpace::Srgb,
    ColorSpace::SrgbLinear,
    ColorSpace::Hsl,
    ColorSpace::Hwb,
    ColorSpace::Lab,
    ColorSpace::Lch,
    ColorSpace::Oklab,
    ColorSpace::Oklch,
    ColorSpace::DisplayP3,
    ColorSpace::A98Rgb,
    ColorSpace::ProphotoRgb,
    ColorSpace::Rec2020,
    ColorSpace::XyzD65,
    ColorSpace::XyzD50,
];

/// sRGB samples and their coordinates in every space, computed with
/// colorjs.io 0.7.1 (by the CSS Color 4 editors). Powerless hues are 0.
/// An sRGB sample and its coordinates in each space.
type Reference = ([f64; 3], &'static [(ColorSpace, [f64; 3])]);

const REFERENCES: &[Reference] = &[
    (
        [0.2, 0.4, 0.9],
        &[
            (ColorSpace::Srgb, [0.2, 0.4, 0.9]),
            (
                ColorSpace::SrgbLinear,
                [
                    0.033104766570885055,
                    0.13286832155381798,
                    0.7874122893956174,
                ],
            ),
            (
                ColorSpace::Hsl,
                [222.85714285714286, 77.77777777777779, 55.00000000000001],
            ),
            (
                ColorSpace::Hwb,
                [222.85714285714286, 20.000000000000004, 10.0],
            ),
            (
                ColorSpace::Lab,
                [45.68208956390008, 18.96454930556779, -71.03510676556952],
            ),
            (
                ColorSpace::Lch,
                [45.68208956390008, 73.5230611683108, 284.94787286726483],
            ),
            (
                ColorSpace::Oklab,
                [
                    0.5518026071036082,
                    -0.020143382880649652,
                    -0.20047488006532233,
                ],
            ),
            (
                ColorSpace::Oklch,
                [0.5518026071036082, 0.20148432547243425, 264.262272720753],
            ),
            (
                ColorSpace::DisplayP3,
                [0.24985133313829072, 0.395240072218916, 0.8688323473100794],
            ),
            (
                ColorSpace::A98Rgb,
                [0.2814316252990777, 0.3994051501298434, 0.8829318770762438],
            ),
            (
                ColorSpace::ProphotoRgb,
                [0.3761814947144651, 0.3374268193233278, 0.8187265198609903],
            ),
            (
                ColorSpace::Rec2020,
                [0.3809174166907831, 0.4320141399110637, 0.8707842013123214],
            ),
            (
                ColorSpace::XyzD65,
                [0.2032765229209613, 0.1589077429439405, 0.7649378507175415],
            ),
            (
                ColorSpace::XyzD50,
                [0.17827199561085255, 0.150349942078824, 0.5756505901809696],
            ),
        ],
    ),
    (
        [1.0, 0.0, 0.0],
        &[
            (ColorSpace::Srgb, [1.0, 0.0, 0.0]),
            (ColorSpace::SrgbLinear, [1.0, 0.0, 0.0]),
            (ColorSpace::Hsl, [0.0, 100.0, 50.0]),
            (ColorSpace::Hwb, [0.0, 0.0, 0.0]),
            (
                ColorSpace::Lab,
                [54.29054140467191, 80.80492817043522, 69.89096476862429],
            ),
            (
                ColorSpace::Lch,
                [54.29054140467191, 106.83718160321469, 40.85765650501162],
            ),
            (
                ColorSpace::Oklab,
                [0.6279553639214311, 0.2248630684262744, 0.125846277330585],
            ),
            (
                ColorSpace::Oklch,
                [0.6279553639214311, 0.2576833038053608, 29.23388027962784],
            ),
            (
                ColorSpace::DisplayP3,
                [0.9174875573251656, 0.20028680774084717, 0.13856059121111408],
            ),
            (
                ColorSpace::A98Rgb,
                [0.8585916022954422, 8.152862362272588e-09, 0.0],
            ),
            (
                ColorSpace::ProphotoRgb,
                [0.7022480752276675, 0.2757205310249222, 0.1035476646501957],
            ),
            (
                ColorSpace::Rec2020,
                [0.8234641019745034, 0.328429446017081, 0.18033932898054666],
            ),
            (
                ColorSpace::XyzD65,
                [
                    0.41239079926595934,
                    0.21263900587151027,
                    0.01933081871559182,
                ],
            ),
            (
                ColorSpace::XyzD50,
                [0.4360657468742693, 0.22249317711056518, 0.01392392146316938],
            ),
        ],
    ),
    (
        [0.5, 0.5, 0.5],
        &[
            (ColorSpace::Srgb, [0.5, 0.5, 0.5]),
            (
                ColorSpace::SrgbLinear,
                [
                    0.21404114048223255,
                    0.21404114048223255,
                    0.21404114048223255,
                ],
            ),
            (ColorSpace::Hsl, [0.0, 0.0, 50.0]),
            (ColorSpace::Hwb, [0.0, 50.0, 50.0]),
            (ColorSpace::Lab, [53.38896474111432, 0.0, 0.0]),
            (ColorSpace::Lch, [53.38896474111432, 0.0, 0.0]),
            (
                ColorSpace::Oklab,
                [
                    0.5981807305268477,
                    -3.3306690738754696e-16,
                    -5.551115123125783e-17,
                ],
            ),
            (ColorSpace::Oklch, [0.5981807305268477, 0.0, 0.0]),
            (
                ColorSpace::DisplayP3,
                [0.49999999999999994, 0.49999999999999994, 0.5],
            ),
            (
                ColorSpace::A98Rgb,
                [0.4961036984487175, 0.49610369844871743, 0.49610369844871743],
            ),
            (
                ColorSpace::ProphotoRgb,
                [0.42467239487818226, 0.4246723948781822, 0.4246723948781822],
            ),
            (
                ColorSpace::Rec2020,
                [0.5260663507109006, 0.5260663507109006, 0.5260663507109006],
            ),
            (
                ColorSpace::XyzD65,
                [
                    0.20343667060423742,
                    0.21404114048223255,
                    0.23310316302365935,
                ],
            ),
            (
                ColorSpace::XyzD50,
                [0.20639894634507058, 0.21404114048223258, 0.1766063301384781],
            ),
        ],
    ),
    (
        [0.95, 0.85, 0.1],
        &[
            (ColorSpace::Srgb, [0.95, 0.85, 0.1]),
            (
                ColorSpace::SrgbLinear,
                [0.8900054069935289, 0.692071056865372, 0.010022825574869039],
            ),
            (ColorSpace::Hsl, [52.94117647058823, 89.4736842105263, 52.5]),
            (
                ColorSpace::Hwb,
                [52.94117647058823, 10.000000000000002, 5.0],
            ),
            (
                ColorSpace::Lab,
                [86.739438643861, -3.077634121646411, 82.74648635880143],
            ),
            (
                ColorSpace::Lch,
                [86.739438643861, 82.80370062089034, 92.13005058585145],
            ),
            (
                ColorSpace::Oklab,
                [
                    0.8788077665897969,
                    -0.03259201702340869,
                    0.17623865518399184,
                ],
            ),
            (
                ColorSpace::Oklch,
                [0.8788077665897969, 0.17922695989921866, 100.4774037776861],
            ),
            (
                ColorSpace::DisplayP3,
                [0.9332716430887548, 0.853570029435135, 0.3024017545592103],
            ),
            (
                ColorSpace::A98Rgb,
                [0.9205839892345062, 0.8458929590874369, 0.2263198009665347],
            ),
            (
                ColorSpace::ProphotoRgb,
                [0.8208684275589081, 0.8152407101923698, 0.286086242606225],
            ),
            (
                ColorSpace::Rec2020,
                [0.9048788161974707, 0.8608733753031237, 0.3571134932199463],
            ),
            (
                ColorSpace::XyzD65,
                [0.6163127402787596, 0.684920979299892, 0.10922280836909372],
            ),
            (
                ColorSpace::XyzD50,
                [0.6560871348649651, 0.6947644652328867, 0.08673683334798761],
            ),
        ],
    ),
];

fn srgb(components: [f64; 3]) -> Color {
    Color::new(ColorSpace::Srgb, components, 1.0)
}

/// Whether two coordinate triples of `space` agree within `tolerance`, a
/// hue counting only where the color has chroma.
fn close(space: ColorSpace, actual: [f64; 3], expected: [f64; 3], tolerance: f64) -> bool {
    let hue = match space {
        ColorSpace::Hsl | ColorSpace::Hwb => Some(0),
        ColorSpace::Lch | ColorSpace::Oklch => Some(2),
        _ => None,
    };
    let achromatic = match space {
        ColorSpace::Hsl => expected[1] < 1e-4,
        ColorSpace::Hwb => expected[1] + expected[2] > 100.0 - 1e-4,
        ColorSpace::Lch | ColorSpace::Oklch => expected[1] < 1e-4,
        _ => false,
    };
    (0..3).all(|index| {
        if Some(index) == hue {
            if achromatic {
                return true;
            }
            let difference = (actual[index] - expected[index]).rem_euclid(360.0);
            difference.min(360.0 - difference) <= tolerance
        } else {
            (actual[index] - expected[index]).abs() <= tolerance
        }
    })
}

fn assert_valid(color: &Color) {
    validate_value_with("color", &color.to_json(), &ValueExtensions::default())
        .unwrap_or_else(|error| panic!("{:?}: {error}", color.to_json()));
}

#[test]
fn conversions_match_css_color_4_reference_values() {
    for (input, expected) in REFERENCES {
        for (space, coordinates) in *expected {
            let converted = srgb(*input).convert(*space);
            assert_eq!(converted.color_space, *space);
            assert!(
                close(*space, converted.coordinates(), *coordinates, 1e-5),
                "{input:?} -> {}: {:?} != {coordinates:?}",
                space.name(),
                converted.coordinates()
            );
            assert_valid(&converted);
            // And back from each space to sRGB.
            let source = Color::new(*space, *coordinates, 1.0);
            assert!(
                close(
                    ColorSpace::Srgb,
                    source.convert(ColorSpace::Srgb).coordinates(),
                    *input,
                    1e-5
                ),
                "{} -> srgb",
                space.name()
            );
        }
    }
}

/// Colors well inside every RGB gamut; the gamut boundaries have a test of
/// their own.
const INTERIOR: [[f64; 3]; 4] = [
    [0.2, 0.4, 0.9],
    [0.5, 0.5, 0.5],
    [0.95, 0.85, 0.1],
    [0.3, 0.6, 0.2],
];

#[test]
fn in_gamut_round_trips_through_every_space_return_the_input() {
    for input in &INTERIOR {
        for through in SPACES {
            for back in SPACES {
                let start = srgb(*input).convert(back);
                let round_trip = start.convert(through).convert(back);
                assert!(
                    close(back, round_trip.coordinates(), start.coordinates(), 1e-9),
                    "{input:?} in {} through {}: {:?} != {:?}",
                    back.name(),
                    through.name(),
                    round_trip.coordinates(),
                    start.coordinates()
                );
            }
        }
    }
}

#[test]
fn none_components_count_as_zero() {
    let color = Color {
        components: [Component::None, Component::Number(0.5), Component::None],
        ..srgb([0.0, 0.0, 0.0])
    };
    assert_eq!(color.coordinates(), [0.0, 0.5, 0.0]);
    assert_eq!(
        color.convert(ColorSpace::Srgb).coordinates(),
        [0.0, 0.5, 0.0]
    );
    let oklch = Color {
        components: [
            Component::Number(0.7),
            Component::Number(0.1),
            Component::None,
        ],
        ..Color::new(ColorSpace::Oklch, [0.0; 3], 1.0)
    };
    let converted = oklch.convert(ColorSpace::Oklab).coordinates();
    assert!(
        (converted[0] - 0.7).abs() < 1e-12
            && (converted[1] - 0.1).abs() < 1e-12
            && converted[2].abs() < 1e-12
    );
}

#[test]
fn results_are_valid_dtcg_colors() {
    // Out of the sRGB gamut, the conversion clips into it.
    let p3_red = Color::new(ColorSpace::DisplayP3, [1.0, 0.0, 0.0], 0.5);
    assert!(!p3_red.in_gamut(ColorSpace::Srgb));
    for space in SPACES {
        let converted = p3_red.convert(space);
        assert_valid(&converted);
        assert_eq!(converted.alpha, 0.5);
    }
    let clipped = p3_red.clamp_to_gamut(ColorSpace::Srgb);
    assert!(clipped.in_gamut(ColorSpace::Srgb));
    assert_eq!(clipped.coordinates()[0], 1.0);
    assert_eq!(clipped.coordinates()[1], 0.0);
    // Hues are normalized.
    let hue =
        Color::new(ColorSpace::Oklch, [0.6, 0.1, 359.9999999999], 1.0).convert(ColorSpace::Lch);
    assert!((0.0..360.0).contains(&hue.coordinates()[2]));
    assert_valid(&hue);
}

#[test]
fn mixing_defaults_to_oklch_with_the_shorter_hue() {
    for (a, b, expected) in [
        (
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5399845410479274, 0.28544884621992284, 326.6429514479988],
        ),
        (
            [0.2, 0.4, 0.9],
            [0.95, 0.85, 0.1],
            [0.7153051868467025, 0.19035564268582644, 182.36983824921955],
        ),
    ] {
        let mixed = srgb(a).mix(&srgb(b), 0.5);
        assert_eq!(mixed.color_space, ColorSpace::Oklch);
        assert!(
            close(ColorSpace::Oklch, mixed.coordinates(), expected, 1e-5),
            "{:?}",
            mixed.coordinates()
        );
        assert_valid(&mixed);
    }
    // Hues 350 and 10 meet at 0, not at 180.
    let first = Color::new(ColorSpace::Oklch, [0.5, 0.1, 350.0], 1.0);
    let second = Color::new(ColorSpace::Oklch, [0.5, 0.1, 10.0], 1.0);
    let mixed = first.mix(&second, 0.5).coordinates();
    assert!(
        mixed[2].abs() < 1e-9 || (mixed[2] - 360.0).abs() < 1e-9,
        "{mixed:?}"
    );
    // The ends of the mix are the colors, in the mixing space; alpha mixes.
    let half = Color::new(ColorSpace::Srgb, [1.0, 0.0, 0.0], 0.0).mix_in(
        &Color::new(ColorSpace::Srgb, [0.0, 0.0, 1.0], 1.0),
        0.5,
        ColorSpace::Srgb,
    );
    assert_eq!(half.alpha, 0.5);
    // Premultiplied: a transparent color contributes no color.
    assert!(
        close(ColorSpace::Srgb, half.coordinates(), [0.0, 0.0, 1.0], 1e-12),
        "{:?}",
        half.coordinates()
    );
    let start = srgb([0.2, 0.4, 0.9]).mix_in(&srgb([1.0, 1.0, 1.0]), 0.0, ColorSpace::Srgb);
    assert!(close(
        ColorSpace::Srgb,
        start.coordinates(),
        [0.2, 0.4, 0.9],
        1e-12
    ));
}

#[test]
fn lighten_and_darken_shift_oklch_lightness_in_the_colors_space() {
    let oklch = Color::new(ColorSpace::Oklch, [0.55, 0.1, 264.0], 1.0);
    assert!(close(
        ColorSpace::Oklch,
        oklch.lighten(0.1).coordinates(),
        [0.65, 0.1, 264.0],
        1e-12
    ));
    assert!(close(
        ColorSpace::Oklch,
        oklch.darken(0.1).coordinates(),
        [0.45, 0.1, 264.0],
        1e-12
    ));
    // In an RGB space the shifted color is clipped into the gamut again.
    let accent = srgb([0.2, 0.4, 0.9]);
    let lighter = accent.lighten(0.1);
    assert_eq!(lighter.color_space, ColorSpace::Srgb);
    let before = accent.convert(ColorSpace::Oklch).coordinates();
    let after = lighter.convert(ColorSpace::Oklch).coordinates();
    assert!(after[0] > before[0] + 0.05, "{before:?} {after:?}");
    let darker = accent.darken(0.1).convert(ColorSpace::Oklch).coordinates();
    assert!((before[0] - darker[0] - 0.1).abs() < 1e-3, "{darker:?}");
    // Lightness stays in [0, 1].
    assert!(close(
        ColorSpace::Srgb,
        accent.lighten(5.0).coordinates(),
        [1.0, 1.0, 1.0],
        1e-9
    ));
    assert!(close(
        ColorSpace::Srgb,
        accent.darken(5.0).coordinates(),
        [0.0, 0.0, 0.0],
        1e-9
    ));
    assert_valid(&accent.lighten(5.0));
}

#[test]
fn alpha_is_replaced_and_clamped() {
    assert_eq!(srgb([0.1, 0.2, 0.3]).with_alpha(0.25).alpha, 0.25);
    assert_eq!(srgb([0.1, 0.2, 0.3]).with_alpha(2.0).alpha, 1.0);
    assert_eq!(srgb([0.1, 0.2, 0.3]).with_alpha(-1.0).alpha, 0.0);
    assert_eq!(
        srgb([0.1, 0.2, 0.3]).with_alpha(0.25).to_json(),
        json!({ "colorSpace": "srgb", "components": [0.1, 0.2, 0.3], "alpha": 0.25 })
    );
}

#[test]
fn hex_converts_both_ways() {
    assert_eq!(srgb([1.0, 0.0, 0.0]).to_hex(), "#ff0000");
    assert_eq!(srgb([0.2, 0.4, 0.9]).to_hex(), "#3366e6");
    assert_eq!(
        Color::new(ColorSpace::Oklch, [1.0, 0.0, 0.0], 1.0).to_hex(),
        "#ffffff"
    );
    // Out-of-gamut colors clip into sRGB.
    assert_eq!(
        Color::new(ColorSpace::DisplayP3, [1.0, 0.0, 0.0], 1.0).to_hex(),
        "#ff0000"
    );
    let parsed = Color::from_hex("#3366E6").unwrap();
    assert_eq!(parsed.color_space, ColorSpace::Srgb);
    assert_eq!(parsed.hex.as_deref(), Some("#3366e6"));
    assert!(close(
        ColorSpace::Srgb,
        parsed.coordinates(),
        [0.2, 0.4, 0.9019607843137255],
        1e-12
    ));
    assert_valid(&parsed);
    for invalid in ["3366e6", "#36e", "#3366e6ff", "#zz66e6", "", "#"] {
        assert!(Color::from_hex(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn contrast_is_the_wcag_2_ratio() {
    for (a, b, expected) in [
        ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 21.0),
        ([0.2, 0.4, 0.9], [1.0, 1.0, 1.0], 5.0261420912568235),
        ([0.5, 0.5, 0.5], [0.95, 0.85, 0.1], 2.7833578432424066),
    ] {
        assert!((srgb(a).contrast(&srgb(b)) - expected).abs() < 1e-9);
        assert!((srgb(b).contrast(&srgb(a)) - expected).abs() < 1e-9);
    }
    assert!((srgb([0.3, 0.3, 0.3]).contrast(&srgb([0.3, 0.3, 0.3])) - 1.0).abs() < 1e-12);
}

#[test]
fn gamut_tests_and_clamping_cover_the_rgb_spaces() {
    let p3 = Color::new(ColorSpace::DisplayP3, [0.0, 1.0, 0.0], 1.0);
    assert!(p3.in_gamut(ColorSpace::DisplayP3));
    assert!(p3.in_gamut(ColorSpace::Rec2020));
    assert!(!p3.in_gamut(ColorSpace::Srgb));
    assert!(srgb([0.2, 0.4, 0.9]).in_gamut(ColorSpace::Srgb));
    assert!(srgb([0.2, 0.4, 0.9]).in_gamut(ColorSpace::DisplayP3));
    // The polar and lab spaces are unbounded.
    assert!(p3.in_gamut(ColorSpace::Oklch));
    for space in [
        ColorSpace::Srgb,
        ColorSpace::SrgbLinear,
        ColorSpace::A98Rgb,
        ColorSpace::Rec2020,
    ] {
        let clamped = p3.clamp_to_gamut(space);
        assert_eq!(clamped.color_space, space);
        assert!(clamped.in_gamut(space));
        assert_valid(&clamped);
    }
}

#[test]
fn colors_serialize_as_dtcg_json() {
    let color = Color::new(ColorSpace::Oklch, [0.6, 0.15, 250.0], 1.0);
    assert_eq!(
        serde_json::to_value(&color).unwrap(),
        json!({ "colorSpace": "oklch", "components": [0.6, 0.15, 250] })
    );
    let parsed: Color = serde_json::from_value(
        json!({ "colorSpace": "srgb", "components": [1, "none", 0], "alpha": 0.5 }),
    )
    .unwrap();
    assert_eq!(parsed.components[1], Component::None);
    assert!(
        serde_json::from_value::<Color>(json!({ "colorSpace": "srgb", "components": [2, 0, 0] }))
            .is_err()
    );
    assert_eq!(
        ColorSpace::from_name("display-p3"),
        Some(ColorSpace::DisplayP3)
    );
    assert_eq!(ColorSpace::from_name("p3"), None);
}

#[test]
fn a_powerless_hue_takes_the_other_colors_hue_when_mixing() {
    // colorjs.io 0.7.1, `Color.mix(a, b, 0.5, { space, hue: "shorter" })`.
    for (space, a, b, expected) in [
        (
            ColorSpace::Oklch,
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.726006859087212, 0.15660719431724246, 264.05202261636987],
        ),
        (
            ColorSpace::Oklch,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.31397768196071557, 0.1288416519026804, 29.23388027962784],
        ),
        (
            ColorSpace::Hsl,
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [240.0, 50.0, 75.0],
        ),
        (
            ColorSpace::Lch,
            [0.5, 0.5, 0.5],
            [0.2, 0.4, 0.9],
            [49.5355271525072, 36.7615305841554, 284.9478728672648],
        ),
    ] {
        let mixed = srgb(a).mix_in(&srgb(b), 0.5, space);
        assert!(
            close(space, mixed.coordinates(), expected, 1e-5),
            "{} {a:?} {b:?}: {:?}",
            space.name(),
            mixed.coordinates()
        );
        // Either order.
        let reversed = srgb(b).mix_in(&srgb(a), 0.5, space);
        assert!(
            close(space, reversed.coordinates(), expected, 1e-5),
            "{:?}",
            reversed.coordinates()
        );
    }
    assert_eq!(
        srgb([1.0, 1.0, 1.0])
            .mix(&srgb([0.0, 0.0, 1.0]), 0.5)
            .to_hex(),
        "#74a3ff"
    );
    assert_eq!(
        srgb([0.0, 0.0, 0.0])
            .mix(&srgb([1.0, 0.0, 0.0]), 0.5)
            .to_hex(),
        "#630000"
    );
}

#[test]
fn conversions_write_powerless_hues_as_zero() {
    for gray in [[0.0; 3], [1.0; 3], [0.5; 3], [0.18; 3]] {
        for space in [
            ColorSpace::Hsl,
            ColorSpace::Hwb,
            ColorSpace::Lch,
            ColorSpace::Oklch,
        ] {
            let converted = srgb(gray).convert(space).coordinates();
            let hue = if matches!(space, ColorSpace::Hsl | ColorSpace::Hwb) {
                0
            } else {
                2
            };
            assert_eq!(
                converted[hue],
                0.0,
                "{gray:?} in {}: {converted:?}",
                space.name()
            );
        }
    }
}

/// The corners of the sRGB cube: primaries, secondaries, black and white,
/// and grays.
const BOUNDARY: [[f64; 3]; 10] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 1.0],
    [1.0, 0.0, 1.0],
    [0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0],
    [0.5, 0.5, 0.5],
    [0.0, 0.5, 1.0],
];

#[test]
fn round_trips_of_gamut_boundary_colors_return_the_input() {
    let p3_green = Color::new(ColorSpace::DisplayP3, [0.0, 1.0, 0.0], 1.0);
    let rec2020_green = Color::new(ColorSpace::Rec2020, [0.0, 1.0, 0.0], 1.0);
    let a98_red = Color::new(ColorSpace::A98Rgb, [1.0, 0.0, 0.0], 1.0);
    let starts = BOUNDARY
        .iter()
        .map(|input| srgb(*input))
        .chain([p3_green, rec2020_green, a98_red])
        .collect::<Vec<_>>();
    for start in &starts {
        for back in SPACES {
            if !start.in_gamut(back) {
                continue;
            }
            let start = start.convert(back);
            // DTCG bounds XYZ to the unit cube: colors outside it are out of
            // those spaces' gamut.
            for through in SPACES
                .into_iter()
                .filter(|through| start.in_gamut(*through))
            {
                let round_trip = start.convert(through).convert(back);
                assert!(
                    close(back, round_trip.coordinates(), start.coordinates(), 1e-9),
                    "{:?} in {} through {}: {:?}",
                    start.coordinates(),
                    back.name(),
                    through.name(),
                    round_trip.coordinates()
                );
            }
        }
    }
}

#[test]
fn colors_on_a_shared_gamut_boundary_are_in_both_gamuts() {
    // A98 RGB shares sRGB's red and blue primaries; Rec. 2020 contains P3.
    for primary in [
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 0.0, 0.0],
    ] {
        assert!(srgb(primary).in_gamut(ColorSpace::A98Rgb), "{primary:?}");
        assert!(srgb(primary).in_gamut(ColorSpace::Rec2020), "{primary:?}");
        assert!(srgb(primary).in_gamut(ColorSpace::DisplayP3), "{primary:?}");
    }
    let a98_red = Color::new(ColorSpace::A98Rgb, [1.0, 0.0, 0.0], 1.0);
    assert!(
        a98_red
            .convert(ColorSpace::Oklch)
            .convert(ColorSpace::A98Rgb)
            .in_gamut(ColorSpace::A98Rgb)
    );
    // sRGB green is outside A98's red-blue edge but inside its gamut.
    assert!(srgb([0.0, 1.0, 0.0]).in_gamut(ColorSpace::A98Rgb));
    assert!(!Color::new(ColorSpace::Rec2020, [0.0, 1.0, 0.0], 1.0).in_gamut(ColorSpace::DisplayP3));
}

#[test]
fn non_finite_inputs_give_valid_colors() {
    let accent = srgb([0.2, 0.4, 0.9]);
    assert_eq!(accent.lighten(f64::NAN), accent.convert(ColorSpace::Srgb));
    assert_eq!(
        accent.darken(f64::INFINITY).coordinates(),
        accent.convert(ColorSpace::Srgb).coordinates()
    );
    assert_eq!(accent.with_alpha(f64::NAN).alpha, 1.0);
    let mixed = accent.mix_in(&srgb([1.0, 1.0, 1.0]), f64::NAN, ColorSpace::Srgb);
    assert!(close(
        ColorSpace::Srgb,
        mixed.coordinates(),
        [0.2, 0.4, 0.9],
        1e-12
    ));
    let built = Color::new(ColorSpace::Oklch, [f64::NAN, -1.0, 400.0], f64::NAN);
    assert_eq!(built.coordinates(), [0.0, 0.0, 40.0]);
    assert_eq!(built.alpha, 1.0);
    for color in [
        accent.lighten(f64::NAN),
        mixed,
        built,
        Color::new(ColorSpace::Srgb, [2.0, -1.0, f64::INFINITY], 2.0),
    ] {
        assert_valid(&color);
    }
}
