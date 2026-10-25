//! Color conversions between the 14 DTCG color spaces with the CSS Color 4
//! formulas, and the operations built on them: mixing, OKLCH lightening and
//! darkening, alpha replacement, sRGB hex, WCAG 2.x contrast, and gamut
//! testing and clamping.
//!
//! Every conversion goes through CIE XYZ relative to D65: the RGB spaces
//! through their transfer functions and primaries, ProPhoto RGB, `xyz-d50`
//! and CIE Lab/LCH through D50 with Bradford chromatic adaptation, OKLab and
//! OKLCH through their LMS matrices, and HSL and HWB through sRGB. The
//! matrices are those of the CSS Color 4 sample code. `none` components
//! count as 0, and every returned color is a valid DTCG color: components
//! are clamped to their space's ranges and hues normalized to `[0, 360)`.

use crate::values::{Color, ColorSpace, Component};

type Vector = [f64; 3];
type Matrix = [Vector; 3];

fn multiply(matrix: &Matrix, vector: Vector) -> Vector {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

const SRGB_TO_XYZ: Matrix = [
    [
        506_752.0 / 1_228_815.0,
        87_881.0 / 245_763.0,
        12_673.0 / 70_218.0,
    ],
    [
        87_098.0 / 409_605.0,
        175_762.0 / 245_763.0,
        12_673.0 / 175_545.0,
    ],
    [
        7_918.0 / 409_605.0,
        87_881.0 / 737_289.0,
        1_001_167.0 / 1_053_270.0,
    ],
];
const XYZ_TO_SRGB: Matrix = [
    [12_831.0 / 3_959.0, -329.0 / 214.0, -1_974.0 / 3_959.0],
    [
        -851_781.0 / 878_810.0,
        1_648_619.0 / 878_810.0,
        36_519.0 / 878_810.0,
    ],
    [705.0 / 12_673.0, -2_585.0 / 12_673.0, 705.0 / 667.0],
];
const P3_TO_XYZ: Matrix = [
    [
        608_311.0 / 1_250_200.0,
        189_793.0 / 714_400.0,
        198_249.0 / 1_000_160.0,
    ],
    [
        35_783.0 / 156_275.0,
        247_089.0 / 357_200.0,
        198_249.0 / 2_500_400.0,
    ],
    [0.0, 32_229.0 / 714_400.0, 5_220_557.0 / 5_000_800.0],
];
const XYZ_TO_P3: Matrix = [
    [
        446_124.0 / 178_915.0,
        -333_277.0 / 357_830.0,
        -72_051.0 / 178_915.0,
    ],
    [-14_852.0 / 17_905.0, 63_121.0 / 35_810.0, 423.0 / 17_905.0],
    [
        11_844.0 / 330_415.0,
        -50_337.0 / 660_830.0,
        316_169.0 / 330_415.0,
    ],
];
const A98_TO_XYZ: Matrix = [
    [
        573_536.0 / 994_567.0,
        263_643.0 / 1_420_810.0,
        187_206.0 / 994_567.0,
    ],
    [
        591_459.0 / 1_989_134.0,
        6_239_551.0 / 9_945_670.0,
        374_412.0 / 4_972_835.0,
    ],
    [
        53_769.0 / 1_989_134.0,
        351_524.0 / 4_972_835.0,
        4_929_758.0 / 4_972_835.0,
    ],
];
const XYZ_TO_A98: Matrix = [
    [
        1_829_569.0 / 896_150.0,
        -506_331.0 / 896_150.0,
        -308_931.0 / 896_150.0,
    ],
    [
        -851_781.0 / 878_810.0,
        1_648_619.0 / 878_810.0,
        36_519.0 / 878_810.0,
    ],
    [
        16_779.0 / 1_248_040.0,
        -147_721.0 / 1_248_040.0,
        1_266_979.0 / 1_248_040.0,
    ],
];
const REC2020_TO_XYZ: Matrix = [
    [
        63_426_534.0 / 99_577_255.0,
        20_160_776.0 / 139_408_157.0,
        47_086_771.0 / 278_816_314.0,
    ],
    [
        26_158_966.0 / 99_577_255.0,
        472_592_308.0 / 697_040_785.0,
        8_267_143.0 / 139_408_157.0,
    ],
    [
        0.0,
        19_567_812.0 / 697_040_785.0,
        295_819_943.0 / 278_816_314.0,
    ],
];
const XYZ_TO_REC2020: Matrix = [
    [
        30_757_411.0 / 17_917_100.0,
        -6_372_589.0 / 17_917_100.0,
        -4_539_589.0 / 17_917_100.0,
    ],
    [
        -19_765_991.0 / 29_648_200.0,
        47_925_759.0 / 29_648_200.0,
        467_509.0 / 29_648_200.0,
    ],
    [
        792_561.0 / 44_930_125.0,
        -1_921_689.0 / 44_930_125.0,
        42_328_811.0 / 44_930_125.0,
    ],
];
/// ProPhoto RGB primaries, relative to D50.
const PROPHOTO_TO_XYZ_D50: Matrix = [
    [
        0.797_766_644_900_642_3,
        0.135_181_297_400_533_08,
        0.031_347_734_128_392_2,
    ],
    [
        0.288_074_828_819_401_3,
        0.711_835_234_241_873,
        0.000_089_936_938_725_64,
    ],
    [0.0, 0.0, 0.825_104_602_510_460_2],
];
const XYZ_D50_TO_PROPHOTO: Matrix = [
    [
        1.345_786_881_647_158_3,
        -0.255_572_087_379_794_64,
        -0.051_101_864_975_545_26,
    ],
    [
        -0.544_630_705_124_901_9,
        1.508_247_742_845_146_8,
        0.020_527_447_436_421_39,
    ],
    [0.0, 0.0, 1.211_967_545_638_945_2],
];
/// Bradford chromatic adaptation between the D65 and D50 white points.
const D65_TO_D50: Matrix = [
    [
        1.047_929_792_544_996_9,
        0.022_946_870_601_609_652,
        -0.050_192_266_289_205_24,
    ],
    [
        0.029_627_808_770_055_99,
        0.990_434_426_753_879_9,
        -0.017_073_799_063_418_826,
    ],
    [
        -0.009_243_040_646_204_504,
        0.015_055_191_490_298_152,
        0.751_874_281_428_137_1,
    ],
];
const D50_TO_D65: Matrix = [
    [
        0.955_473_421_488_075,
        -0.023_098_454_948_764_71,
        0.063_259_243_200_570_72,
    ],
    [
        -0.028_369_709_333_863_7,
        1.009_995_398_081_304_1,
        0.021_041_441_191_917_323,
    ],
    [
        0.012_314_014_864_481_998,
        -0.020_507_649_298_898_964,
        1.330_365_926_242_124,
    ],
];
const XYZ_TO_LMS: Matrix = [
    [
        0.819_022_437_996_703,
        0.361_906_260_052_890_4,
        -0.128_873_781_520_987_9,
    ],
    [
        0.032_983_653_932_388_5,
        0.929_286_861_586_343_4,
        0.036_144_666_350_642_4,
    ],
    [
        0.048_177_189_359_624_2,
        0.264_239_531_752_730_8,
        0.633_547_828_469_430_9,
    ],
];
const LMS_TO_XYZ: Matrix = [
    [
        1.226_879_875_845_924_3,
        -0.557_814_994_460_217_1,
        0.281_391_045_665_964_7,
    ],
    [
        -0.040_575_745_214_800_8,
        1.112_286_803_280_317,
        -0.071_711_058_065_516_4,
    ],
    [
        -0.076_372_936_674_660_1,
        -0.421_493_332_402_243_2,
        1.586_924_019_836_781_6,
    ],
];
const LMS_TO_OKLAB: Matrix = [
    [
        0.210_454_268_309_314,
        0.793_617_774_702_305_4,
        -0.004_072_043_011_619_3,
    ],
    [
        1.977_998_532_431_168_4,
        -2.428_592_242_048_58,
        0.450_593_709_617_411,
    ],
    [
        0.025_904_042_465_547_8,
        0.782_771_712_457_529_6,
        -0.808_675_754_923_077_4,
    ],
];
const OKLAB_TO_LMS: Matrix = [
    [1.0, 0.396_337_777_376_174_9, 0.215_803_757_309_913_6],
    [1.0, -0.105_561_345_815_658_6, -0.063_854_172_825_813_3],
    [1.0, -0.089_484_177_529_811_9, -1.291_485_548_019_409_2],
];

/// The D50 white point, `xyz-d50` of Lab's reference white.
const D50: Vector = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
/// CIE Lab constants: κ = 24389/27 and ε = 216/24389.
const KAPPA: f64 = 24_389.0 / 27.0;
const EPSILON: f64 = 216.0 / 24_389.0;

/// The sRGB transfer function (also Display P3's), from encoded to linear.
fn srgb_linear(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude <= 0.04045 {
        value / 12.92
    } else {
        value.signum() * ((magnitude + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encoded(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude > 0.003_130_8 {
        value.signum() * (1.055 * magnitude.powf(1.0 / 2.4) - 0.055)
    } else {
        12.92 * value
    }
}

fn a98_linear(value: f64) -> f64 {
    value.signum() * value.abs().powf(563.0 / 256.0)
}

/// Linear values this close to 0 are rounding noise of the conversion
/// matrices and roots. The pure power transfer functions (A98 RGB,
/// Rec. 2020) have an infinite slope at 0, which would turn that noise into
/// about 1e-7, so it encodes as 0. In exchange, encoded A98 RGB components
/// below about 4e-7 and Rec. 2020 components below about 1.5e-6 read as 0.
const NOISE: f64 = 1e-14;

fn a98_encoded(value: f64) -> f64 {
    if value.abs() < NOISE {
        return 0.0;
    }
    value.signum() * value.abs().powf(256.0 / 563.0)
}

fn prophoto_linear(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude <= 16.0 / 512.0 {
        value / 16.0
    } else {
        value.signum() * magnitude.powf(1.8)
    }
}

fn prophoto_encoded(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude >= 1.0 / 512.0 {
        value.signum() * magnitude.powf(1.0 / 1.8)
    } else {
        16.0 * value
    }
}

/// Rec. 2020's transfer function as CSS Color 4 defines it: the BT.1886
/// reference EOTF, a pure 2.4 power, extended to negative values.
fn rec2020_linear(value: f64) -> f64 {
    value.signum() * value.abs().powf(2.4)
}

fn rec2020_encoded(value: f64) -> f64 {
    if value.abs() < NOISE {
        return 0.0;
    }
    value.signum() * value.abs().powf(1.0 / 2.4)
}

fn hue(value: f64) -> f64 {
    let hue = value.rem_euclid(360.0);
    if hue >= 360.0 { 0.0 } else { hue }
}

fn hsl_to_srgb([h, s, l]: Vector) -> Vector {
    let (h, s, l) = (hue(h), s / 100.0, l / 100.0);
    let channel = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}

fn srgb_to_hsl([r, g, b]: Vector) -> Vector {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (min + max) / 2.0;
    let delta = max - min;
    let (mut h, mut s) = (0.0, 0.0);
    // Near black, white or gray, saturation is ill-conditioned: rounding
    // noise would decide it.
    if delta > 1e-12 && light > 1e-12 && light < 1.0 - 1e-12 {
        s = if light == 0.0 || light == 1.0 {
            0.0
        } else {
            (max - light) / light.min(1.0 - light)
        };
        h = if max == r {
            (g - b) / delta + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        } * 60.0;
    }
    if s < 0.0 {
        h += 180.0;
        s = s.abs();
    }
    [hue(h), s * 100.0, light * 100.0]
}

fn hwb_to_srgb([h, w, b]: Vector) -> Vector {
    let (w, b) = (w / 100.0, b / 100.0);
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray; 3];
    }
    hsl_to_srgb([h, 100.0, 50.0]).map(|channel| channel * (1.0 - w - b) + w)
}

fn srgb_to_hwb(rgb: Vector) -> Vector {
    let [h, ..] = srgb_to_hsl(rgb);
    let white = rgb[0].min(rgb[1]).min(rgb[2]);
    let black = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    [h, white * 100.0, black * 100.0]
}

fn xyz_d50_to_lab(xyz: Vector) -> Vector {
    let f = [0, 1, 2].map(|index| {
        let value = xyz[index] / D50[index];
        if value > EPSILON {
            value.cbrt()
        } else {
            (KAPPA * value + 16.0) / 116.0
        }
    });
    [
        116.0 * f[1] - 16.0,
        500.0 * (f[0] - f[1]),
        200.0 * (f[1] - f[2]),
    ]
}

fn lab_to_xyz_d50([l, a, b]: Vector) -> Vector {
    let f1 = (l + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let x = if f0.powi(3) > EPSILON {
        f0.powi(3)
    } else {
        (116.0 * f0 - 16.0) / KAPPA
    };
    let y = if l > KAPPA * EPSILON {
        ((l + 16.0) / 116.0).powi(3)
    } else {
        l / KAPPA
    };
    let z = if f2.powi(3) > EPSILON {
        f2.powi(3)
    } else {
        (116.0 * f2 - 16.0) / KAPPA
    };
    [x * D50[0], y * D50[1], z * D50[2]]
}

/// Cartesian `[lightness, a, b]` to cylindrical `[lightness, chroma, hue]`.
fn to_polar([l, a, b]: Vector) -> Vector {
    [l, a.hypot(b), hue(b.atan2(a).to_degrees())]
}

fn from_polar([l, c, h]: Vector) -> Vector {
    let radians = h.to_radians();
    [l, c * radians.cos(), c * radians.sin()]
}

fn xyz_to_oklab(xyz: Vector) -> Vector {
    multiply(&LMS_TO_OKLAB, multiply(&XYZ_TO_LMS, xyz).map(f64::cbrt))
}

fn oklab_to_xyz(oklab: Vector) -> Vector {
    multiply(
        &LMS_TO_XYZ,
        multiply(&OKLAB_TO_LMS, oklab).map(|value| value.powi(3)),
    )
}

/// Coordinates in `space` to CIE XYZ relative to D65, without clamping.
fn to_xyz(space: ColorSpace, c: Vector) -> Vector {
    match space {
        ColorSpace::Srgb => multiply(&SRGB_TO_XYZ, c.map(srgb_linear)),
        ColorSpace::SrgbLinear => multiply(&SRGB_TO_XYZ, c),
        ColorSpace::Hsl => to_xyz(ColorSpace::Srgb, hsl_to_srgb(c)),
        ColorSpace::Hwb => to_xyz(ColorSpace::Srgb, hwb_to_srgb(c)),
        ColorSpace::DisplayP3 => multiply(&P3_TO_XYZ, c.map(srgb_linear)),
        ColorSpace::A98Rgb => multiply(&A98_TO_XYZ, c.map(a98_linear)),
        ColorSpace::Rec2020 => multiply(&REC2020_TO_XYZ, c.map(rec2020_linear)),
        ColorSpace::ProphotoRgb => multiply(
            &D50_TO_D65,
            multiply(&PROPHOTO_TO_XYZ_D50, c.map(prophoto_linear)),
        ),
        ColorSpace::XyzD65 => c,
        ColorSpace::XyzD50 => multiply(&D50_TO_D65, c),
        ColorSpace::Lab => multiply(&D50_TO_D65, lab_to_xyz_d50(c)),
        ColorSpace::Lch => to_xyz(ColorSpace::Lab, from_polar(c)),
        ColorSpace::Oklab => oklab_to_xyz(c),
        ColorSpace::Oklch => oklab_to_xyz(from_polar(c)),
    }
}

/// CIE XYZ relative to D65 to coordinates in `space`, without clamping.
fn from_xyz(space: ColorSpace, xyz: Vector) -> Vector {
    match space {
        ColorSpace::Srgb => multiply(&XYZ_TO_SRGB, xyz).map(srgb_encoded),
        ColorSpace::SrgbLinear => multiply(&XYZ_TO_SRGB, xyz),
        ColorSpace::Hsl => srgb_to_hsl(from_xyz(ColorSpace::Srgb, xyz)),
        ColorSpace::Hwb => srgb_to_hwb(from_xyz(ColorSpace::Srgb, xyz)),
        ColorSpace::DisplayP3 => multiply(&XYZ_TO_P3, xyz).map(srgb_encoded),
        ColorSpace::A98Rgb => multiply(&XYZ_TO_A98, xyz).map(a98_encoded),
        ColorSpace::Rec2020 => multiply(&XYZ_TO_REC2020, xyz).map(rec2020_encoded),
        ColorSpace::ProphotoRgb => {
            multiply(&XYZ_D50_TO_PROPHOTO, multiply(&D65_TO_D50, xyz)).map(prophoto_encoded)
        }
        ColorSpace::XyzD65 => xyz,
        ColorSpace::XyzD50 => multiply(&D65_TO_D50, xyz),
        ColorSpace::Lab => xyz_d50_to_lab(multiply(&D65_TO_D50, xyz)),
        ColorSpace::Lch => to_polar(from_xyz(ColorSpace::Lab, xyz)),
        ColorSpace::Oklab => xyz_to_oklab(xyz),
        ColorSpace::Oklch => to_polar(xyz_to_oklab(xyz)),
    }
}

/// The linear-light coordinates of an RGB or XYZ `space` (sRGB's for HSL and
/// HWB), before its transfer function, from CIE XYZ relative to D65; `None`
/// for the unbounded Lab spaces.
fn linear_from_xyz(space: ColorSpace, xyz: Vector) -> Option<Vector> {
    Some(match space {
        // HSL and HWB describe the sRGB gamut.
        ColorSpace::Srgb | ColorSpace::SrgbLinear | ColorSpace::Hsl | ColorSpace::Hwb => {
            multiply(&XYZ_TO_SRGB, xyz)
        }
        ColorSpace::DisplayP3 => multiply(&XYZ_TO_P3, xyz),
        ColorSpace::A98Rgb => multiply(&XYZ_TO_A98, xyz),
        ColorSpace::Rec2020 => multiply(&XYZ_TO_REC2020, xyz),
        ColorSpace::ProphotoRgb => multiply(&XYZ_D50_TO_PROPHOTO, multiply(&D65_TO_D50, xyz)),
        ColorSpace::XyzD65 => xyz,
        ColorSpace::XyzD50 => multiply(&D65_TO_D50, xyz),
        _ => return None,
    })
}

/// Whether the hue of coordinates in a cylindrical `space` is powerless
/// (CSS Color 4 §4.4): the color has no chroma (no saturation, or is white,
/// black or gray), so its hue carries no information.
fn powerless(space: ColorSpace, c: Vector) -> bool {
    match space {
        ColorSpace::Hsl => c[1].abs() < 1e-6 || c[2] <= 1e-6 || c[2] >= 100.0 - 1e-6,
        ColorSpace::Hwb => c[1] + c[2] >= 100.0 - 1e-6,
        // The thresholds colorjs.io uses: 1e-5 of the a/b axis extent.
        ColorSpace::Lch => c[1] < 0.0025,
        ColorSpace::Oklch => c[1] < 0.000_008,
        _ => false,
    }
}

/// Coordinates with a powerless hue written as 0.
fn settle_hue(space: ColorSpace, mut c: Vector) -> Vector {
    if let Some(index) = hue_index(space)
        && powerless(space, c)
    {
        c[index] = 0.0;
    }
    c
}

/// The sRGB coordinates of a color in sRGB, HSL or HWB.
fn srgb_of(space: ColorSpace, c: Vector) -> Option<Vector> {
    match space {
        ColorSpace::Srgb => Some(c),
        ColorSpace::Hsl => Some(hsl_to_srgb(c)),
        ColorSpace::Hwb => Some(hwb_to_srgb(c)),
        _ => None,
    }
}

/// `value` when it is finite, else `fallback`.
fn finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

/// Whether `space` is bounded by the unit cube: the RGB spaces and XYZ.
fn is_cube(space: ColorSpace) -> bool {
    matches!(
        space,
        ColorSpace::Srgb
            | ColorSpace::SrgbLinear
            | ColorSpace::DisplayP3
            | ColorSpace::A98Rgb
            | ColorSpace::ProphotoRgb
            | ColorSpace::Rec2020
            | ColorSpace::XyzD65
            | ColorSpace::XyzD50
    )
}

/// The index of the hue component of a cylindrical space.
fn hue_index(space: ColorSpace) -> Option<usize> {
    match space {
        ColorSpace::Hsl | ColorSpace::Hwb => Some(0),
        ColorSpace::Lch | ColorSpace::Oklch => Some(2),
        _ => None,
    }
}

/// Clamps coordinates into the component ranges DTCG allows in `space`.
fn into_range(space: ColorSpace, c: Vector) -> Vector {
    let mut c = c.map(|value| finite(value, 0.0));
    if is_cube(space) {
        return c.map(|value| value.clamp(0.0, 1.0));
    }
    match space {
        ColorSpace::Hsl | ColorSpace::Hwb => {
            c[0] = hue(c[0]);
            c[1] = c[1].clamp(0.0, 100.0);
            c[2] = c[2].clamp(0.0, 100.0);
        }
        ColorSpace::Lab => c[0] = c[0].clamp(0.0, 100.0),
        ColorSpace::Lch => {
            c[0] = c[0].clamp(0.0, 100.0);
            c[1] = c[1].max(0.0);
            c[2] = hue(c[2]);
        }
        ColorSpace::Oklab => c[0] = c[0].clamp(0.0, 1.0),
        ColorSpace::Oklch => {
            c[0] = c[0].clamp(0.0, 1.0);
            c[1] = c[1].max(0.0);
            c[2] = hue(c[2]);
        }
        _ => {}
    }
    c
}

impl Color {
    /// A valid color from numeric components and alpha: components are
    /// clamped to the space's ranges and hues normalized, a non-finite
    /// component counts as 0, and a non-finite alpha as 1.
    pub fn new(color_space: ColorSpace, components: [f64; 3], alpha: f64) -> Self {
        Self {
            color_space,
            components: into_range(color_space, components).map(Component::Number),
            alpha: finite(alpha, 1.0).clamp(0.0, 1.0),
            hex: None,
        }
    }

    /// The components as numbers, `none` counting as 0.
    pub fn coordinates(&self) -> [f64; 3] {
        self.components.map(|component| match component {
            Component::Number(value) => value,
            Component::None => 0.0,
        })
    }

    /// The coordinates of this color in `space`, unclamped.
    fn coordinates_in(&self, space: ColorSpace) -> Vector {
        if space == self.color_space {
            return self.coordinates();
        }
        // sRGB, HSL and HWB convert among themselves directly.
        if let (Some(rgb), true) = (
            srgb_of(self.color_space, self.coordinates()),
            matches!(space, ColorSpace::Srgb | ColorSpace::Hsl | ColorSpace::Hwb),
        ) {
            let c = match space {
                ColorSpace::Hsl => srgb_to_hsl(rgb),
                ColorSpace::Hwb => srgb_to_hwb(rgb),
                _ => rgb,
            };
            return settle_hue(space, c);
        }
        settle_hue(
            space,
            from_xyz(space, to_xyz(self.color_space, self.coordinates())),
        )
    }

    fn from_coordinates(space: ColorSpace, coordinates: Vector, alpha: f64) -> Self {
        Self::new(space, coordinates, alpha)
    }

    /// This color in `space`, its components clamped to the space's ranges
    /// (an out-of-gamut color is clipped into an RGB space). The alpha is
    /// kept; the hex fallback is dropped.
    ///
    /// Round trips of in-gamut colors return the input within 1e-9, with
    /// one exception: linear values within 1e-14 of 0 encode as 0, so A98
    /// RGB components below about 4e-7 and Rec. 2020 components below
    /// about 1.5e-6 come back as 0.
    pub fn convert(&self, space: ColorSpace) -> Self {
        Self::from_coordinates(space, self.coordinates_in(space), self.alpha)
    }

    /// Mixes `other` into this color by `amount` (0 is this color, 1 is
    /// `other`) in OKLCH, the CSS `color-mix()` default.
    pub fn mix(&self, other: &Color, amount: f64) -> Self {
        self.mix_in(other, amount, ColorSpace::Oklch)
    }

    /// Mixes `other` into this color by `amount` in `space`, as CSS Color 4
    /// interpolates: premultiplied alpha, the shorter arc between hues, and
    /// a powerless hue (of white, black or a gray) taking the other color's.
    /// The result is in `space`; a non-finite `amount` counts as 0.
    pub fn mix_in(&self, other: &Color, amount: f64, space: ColorSpace) -> Self {
        let amount = finite(amount, 0.0).clamp(0.0, 1.0);
        let (mut first, mut second) = (self.coordinates_in(space), other.coordinates_in(space));
        let hue = hue_index(space);
        if let Some(index) = hue {
            match (powerless(space, first), powerless(space, second)) {
                (true, false) => first[index] = second[index],
                (false, true) => second[index] = first[index],
                _ => {}
            }
            first[index] = self::hue(first[index]);
            second[index] = self::hue(second[index]);
            let difference = second[index] - first[index];
            if difference > 180.0 {
                first[index] += 360.0;
            } else if difference < -180.0 {
                second[index] += 360.0;
            }
        }
        let alpha = self.alpha + (other.alpha - self.alpha) * amount;
        let mut mixed = [0.0; 3];
        for index in 0..3 {
            if Some(index) == hue {
                mixed[index] = first[index] + (second[index] - first[index]) * amount;
            } else {
                let (a, b) = (first[index] * self.alpha, second[index] * other.alpha);
                let premultiplied = a + (b - a) * amount;
                mixed[index] = if alpha == 0.0 {
                    premultiplied
                } else {
                    premultiplied / alpha
                };
            }
        }
        Self::from_coordinates(space, mixed, alpha)
    }

    /// Raises the OKLCH lightness by `amount`, in this color's space. The
    /// lightness stops at 0 (black) and 1 (white), where no chroma remains;
    /// a non-finite `amount` counts as 0.
    pub fn lighten(&self, amount: f64) -> Self {
        let amount = finite(amount, 0.0);
        if amount == 0.0 {
            return self.convert(self.color_space);
        }
        let [l, c, h] = self.coordinates_in(ColorSpace::Oklch);
        let l = (l + amount).clamp(0.0, 1.0);
        let c = if l == 0.0 || l == 1.0 { 0.0 } else { c };
        Color::new(ColorSpace::Oklch, [l, c, h], self.alpha).convert(self.color_space)
    }

    /// Lowers the OKLCH lightness by `amount`, in this color's space; a
    /// non-finite `amount` counts as 0.
    pub fn darken(&self, amount: f64) -> Self {
        self.lighten(-amount)
    }

    /// This color with `alpha` (clamped to `[0, 1]`; a non-finite `alpha`
    /// keeps the current one).
    pub fn with_alpha(&self, alpha: f64) -> Self {
        Self {
            alpha: finite(alpha, self.alpha).clamp(0.0, 1.0),
            ..self.clone()
        }
    }

    /// The 6-digit sRGB hex form (`#rrggbb`) of this color, clipped into
    /// sRGB; alpha is not represented.
    pub fn to_hex(&self) -> String {
        let [r, g, b] = self.convert(ColorSpace::Srgb).coordinates();
        let byte = |value: f64| (value * 255.0).round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    }

    /// Parses a 6-digit sRGB hex color (`#rrggbb`, any case) with alpha 1.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits = hex.strip_prefix('#').filter(|digits| {
            digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })?;
        let channel = |index: usize| {
            u8::from_str_radix(&digits[index..index + 2], 16).map(|value| f64::from(value) / 255.0)
        };
        let color = Self::new(
            ColorSpace::Srgb,
            [channel(0).ok()?, channel(2).ok()?, channel(4).ok()?],
            1.0,
        );
        Some(Self {
            hex: Some(format!("#{}", digits.to_ascii_lowercase())),
            ..color
        })
    }

    /// The WCAG 2.x relative luminance: `Y` in XYZ relative to D65.
    pub fn luminance(&self) -> f64 {
        to_xyz(self.color_space, self.coordinates())[1]
    }

    /// The WCAG 2.x contrast ratio with `other`, from 1 to 21.
    pub fn contrast(&self, other: &Color) -> f64 {
        let (a, b) = (self.luminance().max(0.0), other.luminance().max(0.0));
        let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Whether this color lies inside the gamut of `space`: the unit cube of
    /// an RGB or XYZ space, the sRGB gamut for HSL and HWB (tolerating
    /// rounding of 1e-9, on linear light). CIE Lab, LCH, OKLab and OKLCH are
    /// unbounded and contain every color.
    pub fn in_gamut(&self, space: ColorSpace) -> bool {
        // On linear light: the transfer functions keep `[0, 1]` and would
        // only magnify rounding noise near 0.
        linear_from_xyz(space, to_xyz(self.color_space, self.coordinates())).is_none_or(|linear| {
            linear
                .iter()
                .all(|value| (-1e-9..=1.0 + 1e-9).contains(value))
        })
    }

    /// This color in `space`, clipped into its gamut: [`Color::convert`].
    pub fn clamp_to_gamut(&self, space: ColorSpace) -> Self {
        self.convert(space)
    }
}
