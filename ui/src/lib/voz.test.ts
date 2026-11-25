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
});
