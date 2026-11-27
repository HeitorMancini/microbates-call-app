// Geometria pura da roda de blobs: calcula posições (sem tocar o DOM) e a ordem
// estável dos participantes na roda, a partir da ordem anterior e da lista atual.

import type { Participant } from './types';

export type Modo = 'palco' | 'roda';

/** Centro do corpo do blob, em coordenadas da janela. */
export interface Ponto {
  x: number;
  y: number;
  pequeno: boolean;
}

export interface Retangulo {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Layout {
  blobs: Map<string, Ponto>;
  tela: Retangulo | null;
}

interface Opcoes {
  modo: Modo;
  temTela: boolean;
  largura: number;
  altura: number;
}

// Geometria de referência desenhada a 1100×720.
const L_REF = 1100;
const A_REF = 720;

function arredondar(n: number): number {
  return Math.round(n);
}

function posicoesRoda(ids: string[], o: Opcoes): Map<string, Ponto> {
  const s = Math.min(o.largura / L_REF, o.altura / A_REF);
  const cx = o.largura / 2;
  const cy = o.altura / 2 - 14;
  const blobs = new Map<string, Ponto>();
  const n = ids.length;

  if (n === 1) {
    blobs.set(ids[0], { x: arredondar(cx), y: arredondar(cy), pequeno: false });
    return blobs;
  }

  if (n === 2 && !o.temTela) {
    const dx = 120 * s;
    blobs.set(ids[0], { x: arredondar(cx - dx), y: arredondar(cy), pequeno: false });
    blobs.set(ids[1], { x: arredondar(cx + dx), y: arredondar(cy), pequeno: false });
    return blobs;
  }

  const rxMax = (o.largura - 120) / 2;
  const ryMax = (o.altura - 280) / 2;
  const rxBruto = o.temTela ? 330 * s : (110 + 22 * n) * s;
  const ryBruto = o.temTela ? 222 * s : (90 + 18 * n) * s;
  const rx = Math.min(rxBruto, rxMax);
  const ry = Math.min(ryBruto, ryMax);

  ids.forEach((id, i) => {
    const angulo = Math.PI + (i * 2 * Math.PI) / n;
    blobs.set(id, {
      x: arredondar(cx + rx * Math.cos(angulo)),
      y: arredondar(cy + ry * Math.sin(angulo)),
      pequeno: false,
    });
  });

  return blobs;
}

function posicoesPalco(ids: string[], o: Opcoes): Map<string, Ponto> {
  const cx = o.largura / 2;
  const n = ids.length;
  const passo = n > 1 ? Math.min(84, (o.largura - 420) / (n - 1)) : 0;
  const meio = (n - 1) / 2;
  const blobs = new Map<string, Ponto>();

  ids.forEach((id, i) => {
    blobs.set(id, { x: arredondar(cx + (i - meio) * passo), y: 60, pequeno: true });
  });

  return blobs;
}

function telaDaRoda(o: Opcoes): Retangulo | null {
  if (!o.temTela) return null;
  const s = Math.min(o.largura / L_REF, o.altura / A_REF);
  const cx = o.largura / 2;
  const cy = o.altura / 2 - 14;
  const w = 340 * s;
  const h = 192 * s;
  return { x: arredondar(cx - w / 2), y: arredondar(cy - h / 2), w: arredondar(w), h: arredondar(h) };
}

function telaDoPalco(o: Opcoes): Retangulo | null {
  if (!o.temTela) return null;
  return { x: 40, y: 128, w: Math.max(0, arredondar(o.largura - 80)), h: Math.max(0, arredondar(o.altura - 268)) };
}

export function posicoes(ids: string[], o: Opcoes): Layout {
  if (o.modo === 'palco') {
    return { blobs: posicoesPalco(ids, o), tela: telaDoPalco(o) };
  }
  return { blobs: posicoesRoda(ids, o), tela: telaDaRoda(o) };
}

/** Mantém a ordem anterior (quem sai some, quem entra vai para o fim); o local sempre vem primeiro. */
export function ordemDeChegada(anterior: string[], atuais: Participant[]): string[] {
  const presentes = new Set(atuais.map((p) => p.id));
  const local = atuais.find((p) => p.isLocal);
  const ordem: string[] = [];
  const vistos = new Set<string>();

  if (local) {
    ordem.push(local.id);
    vistos.add(local.id);
  }

  for (const id of anterior) {
    if (id !== local?.id && presentes.has(id)) {
      ordem.push(id);
      vistos.add(id);
    }
  }

  for (const p of atuais) {
    if (!vistos.has(p.id)) {
      ordem.push(p.id);
      vistos.add(p.id);
    }
  }

  return ordem;
}
