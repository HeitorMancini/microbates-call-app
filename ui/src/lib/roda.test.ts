import { describe, expect, it } from 'vitest';
import type { Participant } from './types';
import { ordemDeChegada, posicoes } from './roda';

const p = (over: Partial<Participant> & { id: string }): Participant => ({
  name: over.id,
  muted: false,
  speaking: false,
  sharingScreen: false,
  isLocal: false,
  ...over,
});

const L = 1100,
  A = 720;

describe('posicoes', () => {
  it('1 pessoa fica no centro', () => {
    expect(posicoes(['a'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs.get('a')).toEqual({
      x: 550,
      y: 346,
      pequeno: false,
    });
  });

  it('2 pessoas lado a lado, a primeira à esquerda', () => {
    const b = posicoes(['a', 'b'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs;
    expect(b.get('a')).toMatchObject({ x: 430, y: 346 });
    expect(b.get('b')).toMatchObject({ x: 670, y: 346 });
  });

  it('3+ começam à esquerda e seguem pelo topo (horário)', () => {
    const b = posicoes(['a', 'b', 'c', 'd'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs;
    expect(b.get('a')!.x).toBeCloseTo(550 - (110 + 22 * 4));
    expect(b.get('b')!.y).toBeLessThan(346); // topo
    expect(b.get('b')!.x).toBeCloseTo(550);
  });

  it('palco: fileira pequena em y=60, passo 84, centrada; tela 40,128', () => {
    const r = posicoes(['a', 'b', 'c'], { modo: 'palco', temTela: true, largura: L, altura: A });
    expect([...r.blobs.values()].map((p) => [p.x, p.y, p.pequeno])).toEqual([
      [466, 60, true],
      [550, 60, true],
      [634, 60, true],
    ]);
    expect(r.tela).toEqual({ x: 40, y: 128, w: 1020, h: 452 });
  });

  it('roda com tela: elipse 330×222 e tela 340×192 centrada', () => {
    const r = posicoes(['a', 'b'], { modo: 'roda', temTela: true, largura: L, altura: A });
    expect(r.blobs.get('a')).toMatchObject({ x: 220, y: 346 });
    expect(r.tela).toEqual({ x: 380, y: 250, w: 340, h: 192 });
  });

  it('sem tela, tela é null mesmo no modo palco', () => {
    expect(posicoes(['a'], { modo: 'palco', temTela: false, largura: L, altura: A }).tela).toBeNull();
  });

  it('12 pessoas em 720×520 cabem entre o topo e o rodapé', () => {
    const ids = Array.from({ length: 12 }, (_, i) => `p${i}`);
    for (const p of posicoes(ids, { modo: 'roda', temTela: false, largura: 720, altura: 520 }).blobs.values()) {
      expect(p.y).toBeGreaterThanOrEqual(120);
      expect(p.y).toBeLessThanOrEqual(400);
      expect(p.x).toBeGreaterThanOrEqual(60);
      expect(p.x).toBeLessThanOrEqual(660);
    }
  });

  it('fileira do palco não invade seletor nem botões (720 de largura, 12 pessoas)', () => {
    const ids = Array.from({ length: 12 }, (_, i) => `p${i}`);
    const xs = [...posicoes(ids, { modo: 'palco', temTela: true, largura: 720, altura: 520 }).blobs.values()].map(
      (p) => p.x,
    );
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(210);
    expect(Math.max(...xs)).toBeLessThanOrEqual(720 - 170);
  });
});

describe('ordemDeChegada', () => {
  it('local primeiro, depois na ordem recebida', () => {
    expect(ordemDeChegada([], [p({ id: 'b' }), p({ id: 'eu', isLocal: true }), p({ id: 'c' })])).toEqual([
      'eu',
      'b',
      'c',
    ]);
  });

  it('lista embaralhada não muda a ordem', () => {
    expect(
      ordemDeChegada(['eu', 'b', 'c'], [p({ id: 'c' }), p({ id: 'b' }), p({ id: 'eu', isLocal: true })]),
    ).toEqual(['eu', 'b', 'c']);
  });

  it('quem sai some, quem entra vai para o fim', () => {
    expect(
      ordemDeChegada(['eu', 'b', 'c'], [p({ id: 'd' }), p({ id: 'eu', isLocal: true }), p({ id: 'c' })]),
    ).toEqual(['eu', 'c', 'd']);
  });
});
