// Motor das paisagens ASCII. Lê as cenas de `cenas.json` (guia em CENAS.md) e
// desenha numa grade de caracteres. Cada abertura sorteia os valores [min, max].

export type Faixa = number | [number, number] | string;
export type CorSpec = string | string[];

interface CamadaBase {
  tipo: string;
}

export interface CamadaCeu extends CamadaBase {
  tipo: 'ceu';
  ate?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  velocidade?: number;
}
export interface CamadaEstrelas extends CamadaBase {
  tipo: 'estrelas';
  quantidade?: number;
  ate?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  piscar?: number;
}
export interface CamadaAstro extends CamadaBase {
  tipo: 'astro';
  forma?: 'disco' | 'listrado';
  x?: Faixa;
  y?: Faixa;
  raio?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  limite?: Faixa;
  velocidade?: number;
}
export interface CamadaCordilheira extends CamadaBase {
  tipo: 'cordilheira';
  base?: Faixa;
  relevo?: Faixa;
  escala?: number;
  velocidade?: number;
  crista?: string;
  corpo?: string;
  cor: CorSpec;
  ate?: Faixa;
}
export interface CamadaNevoa extends CamadaBase {
  tipo: 'nevoa';
  y?: Faixa;
  caractere?: string;
  cor: CorSpec;
  densidade?: number;
  velocidade?: number;
}
export interface CamadaAgua extends CamadaBase {
  tipo: 'agua';
  de?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  velocidade?: number;
  reflexo?: string;
}
export interface CamadaCapim extends CamadaBase {
  tipo: 'capim';
  base?: Faixa;
  relevo?: Faixa;
  escala?: number;
  velocidade?: number;
  caracteres?: string;
  cor: CorSpec;
}
export interface CamadaNuvens extends CamadaBase {
  tipo: 'nuvens';
  ate?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  densidade?: number;
  velocidade?: number;
}
export interface CamadaParticulas extends CamadaBase {
  tipo: 'particulas';
  quantidade?: number;
  de?: Faixa;
  ate?: Faixa;
  caracteres?: string;
  cor: CorSpec;
  piscar?: number;
  deriva?: number;
}
export interface CamadaGrade extends CamadaBase {
  tipo: 'grade';
  de?: Faixa;
  linhas?: string;
  verticais?: string;
  velocidade?: number;
}

export type Camada =
  | CamadaCeu
  | CamadaEstrelas
  | CamadaAstro
  | CamadaCordilheira
  | CamadaNevoa
  | CamadaAgua
  | CamadaCapim
  | CamadaNuvens
  | CamadaParticulas
  | CamadaGrade;

export interface Cena {
  nome: string;
  descricao?: string;
  horizonte?: Faixa;
  variacaoMatiz?: Faixa;
  camadas: Camada[];
}

export const TIPOS = [
  'ceu',
  'estrelas',
  'astro',
  'cordilheira',
  'nevoa',
  'agua',
  'capim',
  'nuvens',
  'particulas',
  'grade',
] as const;

export type Put = (x: number, y: number, ch: string, cor: string) => void;
export type Desenhar = (put: Put, t: number) => void;
export type Rnd = () => number;

/** Gerador pseudoaleatório com semente (mulberry32). */
export function semente(seed: number): Rnd {
  let a = seed | 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function ruido(rnd: Rnd): (x: number) => number {
  const v = Array.from({ length: 512 }, rnd);
  return (x) => {
    const i = Math.floor(x);
    const f = x - i;
    const u = f * f * (3 - 2 * f);
    return v[i & 511] * (1 - u) + v[(i + 1) & 511] * u;
  };
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

// --- cores -----------------------------------------------------------------

type Rgb = [number, number, number];

function hexRgb(hex: string): Rgb {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) throw new Error(`Cor inválida: "${hex}" (use #RRGGBB).`);
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function girarMatiz([r, g, b]: Rgb, graus: number): Rgb {
  if (!graus) return [r, g, b];
  // Rotação de matiz no espaço YIQ: preserva a luminosidade.
  const rad = (graus * Math.PI) / 180;
  const c = Math.cos(rad);
  const s = Math.sin(rad);
  const y = 0.299 * r + 0.587 * g + 0.114 * b;
  const i = 0.596 * r - 0.274 * g - 0.322 * b;
  const q = 0.211 * r - 0.523 * g + 0.312 * b;
  const i2 = i * c - q * s;
  const q2 = i * s + q * c;
  return [y + 0.956 * i2 + 0.621 * q2, y - 0.272 * i2 - 0.647 * q2, y - 1.106 * i2 + 1.703 * q2].map((v) =>
    Math.round(Math.min(255, Math.max(0, v))),
  ) as Rgb;
}

/** Degradê pré-calculado: `paleta(t)` com t de 0 (topo) a 1 (base). */
function paleta(spec: CorSpec, matiz: number): (t: number) => string {
  const stops = (Array.isArray(spec) ? spec : [spec]).map((h) => girarMatiz(hexRgb(h), matiz));
  const N = 32;
  const tabela = Array.from({ length: N }, (_, k) => {
    const t = (k / (N - 1)) * (stops.length - 1);
    const i = Math.min(stops.length - 2, Math.floor(t));
    if (stops.length === 1) return `rgb(${stops[0].join(',')})`;
    const f = t - i;
    const c = stops[i].map((v, j) => Math.round(v + (stops[i + 1][j] - v) * f));
    return `rgb(${c.join(',')})`;
  });
  return (t) => tabela[Math.round(clamp01(t) * (N - 1))];
}

// --- valores ---------------------------------------------------------------

const RE_HORIZONTE = /^horizonte\s*(?:([+-])\s*(\d*\.?\d+))?$/;

function resolvedor(rnd: Rnd, horizonte: number) {
  return (v: Faixa | undefined, padrao: number): number => {
    if (v === undefined) return padrao;
    if (typeof v === 'number') return v;
    if (Array.isArray(v)) return v[0] + rnd() * (v[1] - v[0]);
    const m = RE_HORIZONTE.exec(v.trim());
    if (!m) throw new Error(`Valor inválido: "${v}". Use um número, [min, max] ou "horizonte".`);
    const delta = m[2] ? parseFloat(m[2]) * (m[1] === '-' ? -1 : 1) : 0;
    return horizonte + delta;
  };
}

/** Confere a estrutura das cenas, com mensagens que dizem onde está o erro. */
export function validarCenas(dados: unknown): Cena[] {
  const cenas = (dados as { cenas?: unknown })?.cenas;
  if (!Array.isArray(cenas) || cenas.length === 0) throw new Error('cenas.json precisa de uma lista "cenas" com ao menos uma cena.');
  cenas.forEach((c, i) => {
    const onde = `cena ${i + 1}${c?.nome ? ` ("${c.nome}")` : ''}`;
    if (typeof c?.nome !== 'string' || !c.nome.trim()) throw new Error(`${onde}: falta "nome".`);
    if (!Array.isArray(c.camadas) || c.camadas.length === 0) throw new Error(`${onde}: falta a lista "camadas".`);
    c.camadas.forEach((l: { tipo?: string }, j: number) => {
      if (!TIPOS.includes(l?.tipo as (typeof TIPOS)[number]))
        throw new Error(`${onde}, camada ${j + 1}: tipo "${l?.tipo}" desconhecido. Use: ${TIPOS.join(', ')}.`);
    });
  });
  return cenas as Cena[];
}

/** Sorteia uma cena diferente da anterior (quando há mais de uma). */
export function sortearCena(cenas: Cena[], anterior: string | null, rnd: Rnd = Math.random): Cena {
  const opcoes = cenas.length > 1 ? cenas.filter((c) => c.nome !== anterior) : cenas;
  return opcoes[Math.floor(rnd() * opcoes.length) % opcoes.length];
}

// --- camadas ---------------------------------------------------------------

interface Contexto {
  cols: number;
  rows: number;
  rnd: Rnd;
  faixa: ReturnType<typeof resolvedor>;
  matiz: number;
  astro: { x: number; r: number } | null;
}

function chars(s: string | undefined, padrao: string): string[] {
  const lista = [...(s ?? padrao)];
  return lista.length ? lista : [...padrao];
}

function camadaCeu(l: CamadaCeu, cx: Contexto): Desenhar {
  const n = ruido(cx.rnd);
  const ate = Math.round(cx.faixa(l.ate, 1) * cx.rows);
  const cs = chars(l.caracteres, '.:-=');
  const cor = paleta(l.cor, cx.matiz);
  const vel = l.velocidade ?? 0.01;
  return (put, t) => {
    for (let y = 0; y < ate; y++) {
      const c = cor(y / Math.max(1, ate - 1));
      for (let x = 0; x < cx.cols; x++) {
        const v = n(x * 0.15 + y * 3 + t * vel);
        put(x, y, cs[Math.min(cs.length - 1, Math.floor(v * cs.length))], c);
      }
    }
  };
}

function camadaEstrelas(l: CamadaEstrelas, cx: Contexto): Desenhar {
  const ate = cx.faixa(l.ate, 0.5) * cx.rows;
  const cs = chars(l.caracteres, '.*');
  const cor = paleta(l.cor, cx.matiz)(0);
  const piscar = l.piscar ?? 0.08;
  const pontos = Array.from({ length: l.quantidade ?? 50 }, () => [
    Math.floor(cx.rnd() * cx.cols),
    Math.floor(cx.rnd() * ate),
    cx.rnd() * 10,
  ]);
  return (put, t) => {
    for (const [x, y, p] of pontos) {
      const s = Math.sin(t * piscar + p);
      if (s > -0.3) put(x, y, s > 0.7 ? cs[cs.length - 1] : cs[0], cor);
    }
  };
}

function camadaAstro(l: CamadaAstro, cx: Contexto): Desenhar {
  const x0 = cx.faixa(l.x, 0.5) * cx.cols;
  const y0 = cx.faixa(l.y, 0.2) * cx.rows;
  const r = cx.faixa(l.raio, 3);
  const limite = l.limite === undefined ? Infinity : cx.faixa(l.limite, 1) * cx.rows;
  const cs = chars(l.caracteres, '@o');
  const cor = paleta(l.cor, cx.matiz);
  const listrado = l.forma === 'listrado';
  const vel = l.velocidade ?? 0.1;
  cx.astro = { x: x0, r };
  const R = Math.ceil(r);
  const RX = Math.ceil(r * 1.6);
  return (put, t) => {
    for (let dy = -R; dy <= R; dy++) {
      const y = Math.round(y0 + dy);
      if (y >= limite) continue;
      if (listrado && y > y0 - r * 0.2 && (y + Math.floor(t * vel)) % 2 === 0) continue;
      for (let dx = -RX; dx <= RX; dx++) {
        const d = Math.hypot(dx / 1.6, dy);
        if (d > r) continue;
        put(Math.round(x0 + dx), y, d < r * 0.6 ? cs[0] : cs[cs.length - 1], cor((dy + r) / (2 * r)));
      }
    }
  };
}

function camadaCordilheira(l: CamadaCordilheira, cx: Contexto, idx: number): Desenhar {
  const n = ruido(cx.rnd);
  const base = cx.faixa(l.base, 0.6) * cx.rows;
  const amp = cx.faixa(l.relevo, 0.2) * cx.rows;
  const ate = l.ate === undefined ? cx.rows : Math.round(cx.faixa(l.ate, 1) * cx.rows);
  const sc = l.escala ?? 0.08;
  const vel = l.velocidade ?? 0.02;
  const crista = chars(l.crista, '^A');
  const corpo = chars(l.corpo, '#:=+');
  const cor = paleta(l.cor, cx.matiz);
  return (put, t) => {
    for (let x = 0; x < cx.cols; x++) {
      const xx = x + t * vel;
      const top = Math.floor(base - (n(xx * sc) * amp + n(xx * sc * 2.3 + 90) * amp * 0.35));
      for (let y = Math.max(0, top); y < ate; y++) {
        const ch = y === top ? crista[n(xx * 0.5) > 0.5 ? 0 : crista.length - 1] : corpo[(x + y + idx) % corpo.length];
        put(x, y, ch, cor((y - top) / (cx.rows * 0.35)));
      }
    }
  };
}

function camadaNevoa(l: CamadaNevoa, cx: Contexto): Desenhar {
  const off = cx.rnd() * 100;
  const n = ruido(cx.rnd);
  const y = Math.round(cx.faixa(l.y, 0.6) * cx.rows);
  const ch = l.caractere ?? '~';
  const cor = paleta(l.cor, cx.matiz)(0);
  const dens = l.densidade ?? 0.62;
  const vel = l.velocidade ?? 0.03;
  return (put, t) => {
    for (let x = 0; x < cx.cols; x++) if (n(x * 0.2 + t * vel + off) > dens) put(x, y, ch, cor);
  };
}

function camadaAgua(l: CamadaAgua, cx: Contexto): Desenhar {
  const de = Math.round(cx.faixa(l.de, 0.55) * cx.rows);
  const cs = chars(l.caracteres, '~-._');
  const cor = paleta(l.cor, cx.matiz);
  const refl = l.reflexo ? paleta(l.reflexo, cx.matiz)(0) : null;
  const vel = l.velocidade ?? 0.12;
  const pick = (w: number) => cs[Math.min(cs.length - 1, w > 0.9 ? 0 : w > 0 ? 1 : w > -0.9 ? 2 : 3)];
  return (put, t) => {
    const profundidade = Math.max(1, cx.rows - de);
    for (let y = de; y < cx.rows; y++) {
      const d = y - de;
      const c = cor(d / profundidade);
      for (let x = 0; x < cx.cols; x++) {
        const w = Math.sin(x * 0.35 - t * vel + d * 0.9) + Math.sin(x * 0.13 + t * vel * 0.6 + d * 1.7);
        const brilho =
          refl && cx.astro && Math.abs(x - cx.astro.x) < cx.astro.r * 1.4 * (1 - (d / profundidade) * 0.4) && w > -0.2;
        put(x, y, pick(w), brilho ? refl : c);
      }
    }
  };
}

function camadaCapim(l: CamadaCapim, cx: Contexto, idx: number): Desenhar {
  const n = ruido(cx.rnd);
  const base = cx.faixa(l.base, 0.6) * cx.rows;
  const amp = cx.faixa(l.relevo, 0.17) * cx.rows;
  const sc = l.escala ?? 0.08;
  const vel = l.velocidade ?? 0.18;
  const [dir, esq, parado, ponta] = chars(l.caracteres, '/\\|,').concat(['/', '\\', '|', ',']);
  const cor = paleta(l.cor, cx.matiz);
  return (put, t) => {
    for (let x = 0; x < cx.cols; x++) {
      const top = Math.floor(base - n(x * sc + idx * 30) * amp);
      for (let y = Math.max(0, top); y < cx.rows; y++) {
        const v = Math.sin(x * 0.22 - t * vel + y * 0.15) + (n(x * 0.1 - t * vel * 0.28) - 0.5) * 2;
        const ch = v > 0.6 ? dir : v < -0.6 ? esq : y === top ? ponta : parado;
        put(x, y, ch, cor(v > 0.6 ? 0 : (y - top) / (cx.rows * 0.4)));
      }
    }
  };
}

function camadaNuvens(l: CamadaNuvens, cx: Contexto): Desenhar {
  const n = ruido(cx.rnd);
  const ate = Math.round(cx.faixa(l.ate, 0.35) * cx.rows);
  const cs = chars(l.caracteres, '-=%');
  const cor = paleta(l.cor, cx.matiz)(0);
  const dens = l.densidade ?? 0.32;
  const vel = l.velocidade ?? 0.02;
  return (put, t) => {
    for (let y = 0; y < ate; y++)
      for (let x = 0; x < cx.cols; x++) {
        const c = n(x * 0.12 - t * vel + y * 0.4) * n(x * 0.05 + y * 1.3 + 40);
        if (c > dens) put(x, y, cs[Math.min(cs.length - 1, Math.floor(((c - dens) / 0.2) * cs.length))], cor);
      }
  };
}

function camadaParticulas(l: CamadaParticulas, cx: Contexto): Desenhar {
  const de = cx.faixa(l.de, 0.4) * cx.rows;
  const ate = cx.faixa(l.ate, 1) * cx.rows;
  const cs = chars(l.caracteres, '+*');
  const cor = paleta(l.cor, cx.matiz)(0);
  const piscar = l.piscar ?? 0.12;
  const deriva = l.deriva ?? 3;
  const pontos = Array.from({ length: l.quantidade ?? 14 }, () => [
    cx.rnd() * cx.cols,
    de + cx.rnd() * (ate - de),
    cx.rnd() * 10,
  ]);
  return (put, t) => {
    for (const [x, y, p] of pontos) {
      const b = Math.sin(t * piscar + p);
      if (b > 0.2)
        put(
          Math.round(x + Math.sin(t * 0.03 + p) * deriva),
          Math.round(y + Math.cos(t * 0.04 + p) * deriva * 0.66),
          b > 0.7 ? cs[cs.length - 1] : cs[0],
          cor,
        );
    }
  };
}

function camadaGrade(l: CamadaGrade, cx: Contexto): Desenhar {
  const de = Math.round(cx.faixa(l.de, 0.6) * cx.rows);
  const linhas = paleta(l.linhas ?? '#ff3dc8', cx.matiz)(0);
  const verticais = paleta(l.verticais ?? '#22e6ff', cx.matiz)(0);
  const vel = l.velocidade ?? 0.25;
  const k = cx.rows * 0.64;
  return (put, t) => {
    for (let y = de; y < cx.rows; y++) {
      const d = y - de + 0.6;
      const linha = Math.floor(k / d + t * vel) !== Math.floor(k / (d + 1) + t * vel);
      for (let x = 0; x < cx.cols; x++) {
        const u = ((x - cx.cols / 2) * 1.2) / d;
        if (linha) put(x, y, '=', linhas);
        else if (Math.abs(u - Math.round(u)) < 0.5 / d + 0.04) put(x, y, u < -0.2 ? '/' : u > 0.2 ? '\\' : '|', verticais);
        else put(x, y, ' ', '');
      }
    }
  };
}

/** Prepara uma abertura da cena: sorteia todos os valores e devolve o desenhista. */
export function prepararCena(cena: Cena, cols: number, rows: number, rnd: Rnd): Desenhar {
  const horizonte = resolvedor(rnd, 0.55)(cena.horizonte, 0.55);
  const faixa = resolvedor(rnd, horizonte);
  const cx: Contexto = { cols, rows, rnd, faixa, matiz: faixa(cena.variacaoMatiz, 0), astro: null };
  const passos = cena.camadas.map((l, i): Desenhar => {
    switch (l.tipo) {
      case 'ceu':
        return camadaCeu(l, cx);
      case 'estrelas':
        return camadaEstrelas(l, cx);
      case 'astro':
        return camadaAstro(l, cx);
      case 'cordilheira':
        return camadaCordilheira(l, cx, i);
      case 'nevoa':
        return camadaNevoa(l, cx);
      case 'agua':
        return camadaAgua(l, cx);
      case 'capim':
        return camadaCapim(l, cx, i);
      case 'nuvens':
        return camadaNuvens(l, cx);
      case 'particulas':
        return camadaParticulas(l, cx);
      case 'grade':
        return camadaGrade(l, cx);
    }
  });
  return (put, t) => {
    for (const passo of passos) passo(put, t);
  };
}
