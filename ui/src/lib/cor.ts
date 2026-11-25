// Cor de destaque derivada do blob: mantém o matiz e leva ao ponto mais vivo
// que esse matiz alcança no sRGB (o "cusp" do gamut em OKLCh).

type Rgb = [number, number, number];

const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gam = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

export interface Oklch {
  L: number;
  C: number;
  H: number;
}

function hexToRgbLinear(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => lin(v / 255)) as Rgb;
}

export function hexToOklch(hex: string): Oklch {
  const [r, g, b] = hexToRgbLinear(hex);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { L, C: Math.hypot(A, B), H: ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360 };
}

function oklchToRgbLinear({ L, C, H }: Oklch): Rgb {
  const a = C * Math.cos((H * Math.PI) / 180);
  const b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const inGamut = (rgb: Rgb) => rgb.every((v) => v >= -0.0005 && v <= 1.0005);

function toHex(rgb: Rgb): string {
  return (
    '#' +
    rgb
      .map((v) =>
        Math.round(clamp01(gam(clamp01(v))) * 255)
          .toString(16)
          .padStart(2, '0'),
      )
      .join('')
      .toUpperCase()
  );
}

function luminance(hex: string): number {
  const [r, g, b] = hexToRgbLinear(hex);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Maior croma que o matiz `H` alcança dentro do sRGB, e em que luminosidade. */
function cusp(H: number): { L: number; C: number } {
  let best = { L: 0.7, C: 0 };
  for (let i = 0; i <= 35; i++) {
    const L = 0.55 + i * 0.01;
    let lo = 0;
    let hi = 0.4;
    for (let k = 0; k < 18; k++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToRgbLinear({ L, C: mid, H }))) lo = mid;
      else hi = mid;
    }
    if (lo > best.C) best = { L, C: lo };
  }
  return best;
}

export interface Destaque {
  /** Cor de destaque em hex (#RRGGBB). */
  cor: string;
  /** Cor do texto sobre o destaque, escolhida por contraste WCAG. */
  texto: string;
  oklch: Oklch;
}

export function destaqueDe(hexBlob: string): Destaque {
  const { H } = hexToOklch(hexBlob);
  const { L, C } = cusp(H);
  const oklch = { L, C: C * 0.99, H };
  const cor = toHex(oklchToRgbLinear(oklch));
  const lum = luminance(cor);
  const escuro = (lum + 0.05) / 0.05 >= 1.05 / (lum + 0.05);
  return { cor, texto: escuro ? '#0b0b0c' : '#ffffff', oklch };
}

/** Variação do destaque (mais clara/escura, menos saturada) que continua no gamut. */
export function tom(d: Destaque, dL: number, fC: number): string {
  const L = Math.min(0.97, d.oklch.L + dL);
  let C = d.oklch.C * fC;
  while (C > 0 && !inGamut(oklchToRgbLinear({ L, C, H: d.oklch.H }))) C -= 0.005;
  return toHex(oklchToRgbLinear({ L, C: Math.max(0, C), H: d.oklch.H }));
}

/** Cor do corpo do blob: o primeiro `<g fill>` do SVG do blobatar. */
export function corDoBlob(svg: string): string {
  return svg.match(/<g fill="(#[0-9a-fA-F]{6})"/)?.[1] ?? '#888888';
}
