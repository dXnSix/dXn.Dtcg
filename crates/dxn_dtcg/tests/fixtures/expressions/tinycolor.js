// A small single-file color library written for the expression tests, in
// the style of libraries such as culori: module-level constants, closures,
// classes, `Math` functions and re-exports of internal helpers.

const D65 = [0.3127 / 0.329, 1.0, (1.0 - 0.3127 - 0.329) / 0.329];
const EPSILON = 1e-9;

const clamp01 = (x) => Math.min(1, Math.max(0, x));
const round = (x, digits = 6) => {
  const factor = 10 ** digits;
  return Math.round(x * factor) / factor;
};

function linearize(c) {
  const abs = Math.abs(c);
  return abs <= 0.04045 ? c / 12.92 : Math.sign(c) * ((abs + 0.055) / 1.055) ** 2.4;
}

function gammaEncode(c) {
  const abs = Math.abs(c);
  return abs > 0.0031308 ? Math.sign(c) * (1.055 * abs ** (1 / 2.4) - 0.055) : 12.92 * c;
}

const multiply = (matrix) => (vector) =>
  matrix.map((row) => row.reduce((sum, value, index) => sum + value * vector[index], 0));

const LINEAR_SRGB_TO_LMS = multiply([
  [0.4122214708, 0.5363325363, 0.0514459929],
  [0.2119034982, 0.6806995451, 0.1073969566],
  [0.0883024619, 0.2817188376, 0.6299787005],
]);
const LMS_TO_OKLAB = multiply([
  [0.2104542553, 0.793617785, -0.0040720468],
  [1.9779984951, -2.428592205, 0.4505937099],
  [0.0259040371, 0.7827717662, -0.808675766],
]);
const OKLAB_TO_LMS = multiply([
  [1, 0.3963377774, 0.2158037573],
  [1, -0.1055613458, -0.0638541728],
  [1, -0.0894841775, -1.291485548],
]);
const LMS_TO_LINEAR_SRGB = multiply([
  [4.0767416621, -3.3077115913, 0.2309699292],
  [-1.2684380046, 2.6097574011, -0.3413193965],
  [-0.0041960863, -0.7034186147, 1.707614701],
]);

export class Color {
  #alpha;

  constructor(r, g, b, alpha = 1) {
    this.r = r;
    this.g = g;
    this.b = b;
    this.#alpha = alpha;
  }

  static fromToken(token) {
    if (token.colorSpace !== "srgb") {
      throw new TypeError(`unsupported color space ${token.colorSpace}`);
    }
    const [r, g, b] = token.components;
    return new Color(r, g, b, token.alpha ?? 1);
  }

  get alpha() {
    return this.#alpha;
  }

  oklch() {
    const lms = LINEAR_SRGB_TO_LMS([this.r, this.g, this.b].map(linearize)).map(Math.cbrt);
    const [l, a, b] = LMS_TO_OKLAB(lms);
    const chroma = Math.hypot(a, b);
    const hue = chroma < EPSILON ? 0 : ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360;
    return { l, c: chroma, h: hue };
  }

  static fromOklch({ l, c, h }, alpha = 1) {
    const radians = (h * Math.PI) / 180;
    const lms = OKLAB_TO_LMS([l, c * Math.cos(radians), c * Math.sin(radians)]).map((x) => x ** 3);
    const [r, g, b] = LMS_TO_LINEAR_SRGB(lms).map(gammaEncode).map(clamp01);
    return new Color(r, g, b, alpha);
  }

  toToken() {
    const token = {
      colorSpace: "srgb",
      components: [this.r, this.g, this.b].map((c) => round(c)),
    };
    if (this.#alpha !== 1) token.alpha = this.#alpha;
    return token;
  }
}

export function lighten(token, amount) {
  const color = Color.fromToken(token);
  const { l, c, h } = color.oklch();
  return Color.fromOklch({ l: clamp01(l + amount), c, h }, color.alpha).toToken();
}

export function mix(first, second, weight = 0.5) {
  const a = Color.fromToken(first);
  const b = Color.fromToken(second);
  const lerp = (x, y) => x + (y - x) * weight;
  return new Color(lerp(a.r, b.r), lerp(a.g, b.g), lerp(a.b, b.b), lerp(a.alpha, b.alpha)).toToken();
}

export const luminance = (token) => {
  const [r, g, b] = Color.fromToken(token).toToken().components.map(linearize);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

export const whitePoint = Object.freeze([...D65]);

export { clamp01 as clamp, round as roundTo };
export default Color;
