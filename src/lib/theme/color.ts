// Minimal OKLCH color math for deriving palettes from a single seed color.

export type RGB = [number, number, number];
export type OKLCH = [number, number, number];

export function hexToRgb(hex: string): RGB {
  let h = hex.replace('#', '').trim();
  if (h.length === 3) h = [...h].map((c) => c + c).join('');
  const n = parseInt(h.slice(0, 6), 16);
  if (Number.isNaN(n)) return [122, 124, 240];
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: RGB): string {
  const c = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0');
  return `#${c(r)}${c(g)}${c(b)}`;
}

const toLinear = (c: number) => {
  c /= 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
const fromLinear = (c: number) => 255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

export function rgbToOklch([r, g, b]: RGB): OKLCH {
  const [lr, lg, lb] = [toLinear(r), toLinear(g), toLinear(b)];
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  const C = Math.sqrt(A * A + B * B);
  let H = (Math.atan2(B, A) * 180) / Math.PI;
  if (H < 0) H += 360;
  return [L, C, H];
}

export function oklchToRgb([L, C, H]: OKLCH): RGB {
  const hr = (H * Math.PI) / 180;
  const A = C * Math.cos(hr);
  const B = C * Math.sin(hr);
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return [
    fromLinear(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    fromLinear(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    fromLinear(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

export function shade(hex: string, l: number, cMul = 1): string {
  const [, c, h] = rgbToOklch(hexToRgb(hex));
  return rgbToHex(oklchToRgb([l, Math.min(0.37, c * cMul), h]));
}

export function withAlpha(hex: string, a: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgb(${r} ${g} ${b} / ${a})`;
}

/** Perceived lightness (0..1). */
export function lightness(hex: string): number {
  return rgbToOklch(hexToRgb(hex))[0];
}

/** Pastel chip colors for a hue, adapted to light/dark themes. */
export function pastel(hue: number, dark: boolean): { bg: string; ink: string; dot: string } {
  if (dark) {
    return {
      bg: rgbToHexA(oklchToRgb([0.36, 0.06, hue]), 0.55),
      ink: rgbToHex(oklchToRgb([0.86, 0.08, hue])),
      dot: rgbToHex(oklchToRgb([0.72, 0.12, hue])),
    };
  }
  return {
    bg: rgbToHex(oklchToRgb([0.95, 0.035, hue])),
    ink: rgbToHex(oklchToRgb([0.42, 0.09, hue])),
    dot: rgbToHex(oklchToRgb([0.72, 0.12, hue])),
  };
}

function rgbToHexA(rgb: RGB, a: number): string {
  const [r, g, b] = rgb.map((v) => Math.round(Math.min(255, Math.max(0, v))));
  return `rgb(${r} ${g} ${b} / ${a})`;
}

/** Colors for a user-chosen hex (tags or lanes). */
export function tintFromHex(hex: string, dark: boolean): { bg: string; ink: string; dot: string } {
  const [, c, h] = rgbToOklch(hexToRgb(hex));
  const p = pastel(h, dark);
  if (c < 0.02) {
    return dark ? { bg: 'rgb(255 255 255 / 0.08)', ink: '#d0d0dc', dot: '#9a9aae' } : { bg: '#efeff4', ink: '#55556a', dot: '#9a9aae' };
  }
  return p;
}
