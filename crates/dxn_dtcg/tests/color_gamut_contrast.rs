//! HSL and HWB clip into sRGB, contrast is measured on sRGB, and near-gray
//! colors keep their hue through cylindrical spaces.

use dxn_dtcg::values::{Color, ColorSpace};

#[test]
fn hsl_and_hwb_clip_into_srgb() {
    let red = Color::new(ColorSpace::DisplayP3, [1.0, 0.0, 0.0], 1.0);
    for space in [ColorSpace::Hsl, ColorSpace::Hwb] {
        let converted = red.convert(space);
        assert_eq!(converted.to_hex(), "#ff0000", "{}", space.name());
        assert!(converted.in_gamut(ColorSpace::Srgb), "{}", space.name());
        assert_eq!(red.clamp_to_gamut(space).to_hex(), "#ff0000");
    }
}

#[test]
fn contrast_stays_between_1_and_21() {
    let black = Color::new(ColorSpace::Srgb, [0.0; 3], 1.0);
    let bright = Color::new(ColorSpace::Oklab, [1.0, -0.3, 0.0], 1.0);
    let contrast = bright.contrast(&black);
    assert!((1.0..=21.0).contains(&contrast), "{contrast}");
    let white = Color::new(ColorSpace::Srgb, [1.0; 3], 1.0);
    assert!((white.contrast(&black) - 21.0).abs() < 1e-9);
}

#[test]
fn near_gray_colors_round_trip_through_cylindrical_spaces() {
    for rgb in [
        [0.5, 0.5, 0.500_001],
        [0.2, 0.200_002, 0.2],
        [0.9, 0.9, 0.899_999],
    ] {
        let color = Color::new(ColorSpace::Srgb, rgb, 1.0);
        for space in [
            ColorSpace::Lch,
            ColorSpace::Oklch,
            ColorSpace::Hsl,
            ColorSpace::Hwb,
        ] {
            let back = color.convert(space).convert(ColorSpace::Srgb).coordinates();
            for (actual, expected) in back.iter().zip(rgb) {
                assert!(
                    (actual - expected).abs() < 1e-9,
                    "{rgb:?} via {}: {back:?}",
                    space.name()
                );
            }
        }
    }
}
