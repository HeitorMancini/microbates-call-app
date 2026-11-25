import { describe, expect, it } from 'vitest';
import { criarEnvelope } from './voz';

describe('criarEnvelope', () => {
  it('fica entre 0.05 e 1 e varia com o tempo', () => {
    const nivel = criarEnvelope();
    const vals = Array.from({ length: 200 }, (_, i) => nivel(i * 16));
    vals.forEach((v) => {
      expect(v).toBeGreaterThanOrEqual(0.05);
      expect(v).toBeLessThanOrEqual(1);
    });
    expect(new Set(vals.map((v) => v.toFixed(2))).size).toBeGreaterThan(5);
  });

  it('sorteio baixo vira pausa', () => {
    expect(criarEnvelope(() => 0)(500)).toBeCloseTo(0.05);
  });

  // `aleatorio` controlado: 1ª rodada sem pausa com nível máximo e sorteio de
  // intervalo no teto (220 ms); 2ª rodada vira pausa. Avança ms a ms (em vez de
  // pular direto ao instante do reagendamento) para capturar a interpolação de
  // verdade: se o teto do intervalo regredir (ex.: para 130 ms), o nível já
  // teria caído bem antes de 219 ms, quebrando as asserções abaixo.
  it('reagenda o alvo no máximo 220 ms (e no mínimo 90 ms) após o anterior', () => {
    const valoresMax = [0.9, 1, 1, 0, 0];
    let i = 0;
    const aleatorioMax = () => valoresMax[Math.min(i++, valoresMax.length - 1)];
    const nivelMax = criarEnvelope(aleatorioMax);
    const valsMax = Array.from({ length: 301 }, (_, t) => nivelMax(t));

    expect(valsMax[150]).toBeCloseTo(1, 1); // ainda dentro do 1º intervalo (220 ms)
    expect(valsMax[219]).toBeCloseTo(1, 1); // ainda dentro do 1º intervalo
    expect(valsMax[290]).toBeCloseTo(0.05, 1); // reagendou aos 220 ms e já interpolou a pausa

    const valoresMin = [0.9, 1, 0, 0, 0];
    let j = 0;
    const aleatorioMin = () => valoresMin[Math.min(j++, valoresMin.length - 1)];
    const nivelMin = criarEnvelope(aleatorioMin);
    const valsMin = Array.from({ length: 201 }, (_, t) => nivelMin(t));

    expect(valsMin[89]).toBeCloseTo(1, 1); // ainda dentro do 1º intervalo (90 ms)
    expect(valsMin[150]).toBeCloseTo(0.05, 1); // reagendou aos 90 ms e já interpolou a pausa
  });
});
