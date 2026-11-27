import { describe, expect, it } from 'vitest';
import { LIMIAR_MAX_DB, LIMIAR_MIN_DB, nivelParaFracao, thresholdDbfs } from './micProcessing';

describe('thresholdDbfs (mesma tabela do NoiseGate em Rust)', () => {
  it('100 deixa o gate aberto', () => {
    expect(thresholdDbfs(100)).toBeNull();
    expect(thresholdDbfs(150)).toBeNull();
  });

  it.each([
    [99, -59.6],
    [75, -50],
    [50, -40],
    [0, -20],
  ])('sensibilidade %i → %f dBFS', (sensibilidade, esperado) => {
    expect(thresholdDbfs(sensibilidade)).toBeCloseTo(esperado, 5);
  });

  it('nunca sai da faixa do medidor', () => {
    expect(thresholdDbfs(-10)).toBe(LIMIAR_MAX_DB);
    expect(thresholdDbfs(99)!).toBeGreaterThanOrEqual(LIMIAR_MIN_DB);
  });
});

describe('nivelParaFracao', () => {
  it('mapeia -60..0 dBFS para 0..1 e limita as pontas', () => {
    expect(nivelParaFracao(-60)).toBe(0);
    expect(nivelParaFracao(-30)).toBeCloseTo(0.5);
    expect(nivelParaFracao(0)).toBe(1);
    expect(nivelParaFracao(-100)).toBe(0);
    expect(nivelParaFracao(6)).toBe(1);
    expect(nivelParaFracao(Number.NaN)).toBe(0);
  });
});
