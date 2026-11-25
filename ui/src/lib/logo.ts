// Logo μChat: um μ em serifa itálica rasterizado e redesenhado com caracteres
// ASCII pela densidade do traço (opção A do design), seguido de "Chat".

export interface GradeMu {
  /** Cobertura 0..1 de cada célula, linha a linha. */
  cov: Float32Array;
  cols: number;
  rows: number;
  /** Largura/altura em px da área do μ. */
  w: number;
  h: number;
  /** Linha de base do μ, em px a partir do topo. */
  base: number;
}

export const RAMPA = ' .:-=+*#%@';
export const FONTE_MU = (px: number) => `italic 700 ${px}px Georgia, "Times New Roman", serif`;
const MONO = 'ui-monospace, "Cascadia Mono", Consolas, monospace';

export function rasterizarMu(px: number, cw: number, ch: number): GradeMu | null {
  const medida = document.createElement('canvas').getContext('2d');
  if (!medida) return null;
  medida.font = FONTE_MU(px);
  const t = medida.measureText('μ');
  const pad = Math.ceil(cw * 2);
  const cols = Math.ceil((Math.ceil(t.actualBoundingBoxLeft + t.actualBoundingBoxRight) + pad * 2) / cw);
  const rows = Math.ceil((Math.ceil(t.actualBoundingBoxAscent + t.actualBoundingBoxDescent) + pad * 2) / ch);
  const tela = document.createElement('canvas');
  tela.width = cols * cw;
  tela.height = rows * ch;
  const ctx = tela.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;
  ctx.font = FONTE_MU(px);
  ctx.fillStyle = '#fff';
  const base = pad + t.actualBoundingBoxAscent;
  ctx.fillText('μ', pad + t.actualBoundingBoxLeft, base);
  const d = ctx.getImageData(0, 0, tela.width, tela.height).data;
  const cov = new Float32Array(cols * rows);
  for (let r = 0; r < rows; r++)
    for (let q = 0; q < cols; q++) {
      let s = 0;
      for (let y = 0; y < ch; y++) for (let x = 0; x < cw; x++) s += d[((r * ch + y) * tela.width + q * cw + x) * 4 + 3];
      cov[r * cols + q] = s / (cw * ch * 255);
    }
  return { cov, cols, rows, w: cols * cw, h: rows * ch, base };
}

/** Índice na RAMPA para uma cobertura; `cintilar` desloca alguns caracteres ao acaso. */
export function caractere(v: number, cintilar = 0, rnd = Math.random): string {
  let k = Math.round(v * (RAMPA.length - 1));
  if (cintilar && rnd() < cintilar) k = Math.max(1, Math.min(RAMPA.length - 1, k + (rnd() < 0.5 ? -2 : 2)));
  return RAMPA[k];
}

export function desenharMu(ctx: CanvasRenderingContext2D, g: GradeMu, cw: number, ch: number, cor: string, cintilar = 0) {
  ctx.font = `700 ${Math.round(ch * 1.02)}px ${MONO}`;
  ctx.textBaseline = 'top';
  ctx.fillStyle = cor;
  for (let r = 0; r < g.rows; r++)
    for (let q = 0; q < g.cols; q++) {
      const v = g.cov[r * g.cols + q];
      if (v < 0.08) continue;
      ctx.globalAlpha = 0.35 + 0.65 * v;
      ctx.fillText(caractere(v, cintilar), q * cw, r * ch - 1);
    }
  ctx.globalAlpha = 1;
}
