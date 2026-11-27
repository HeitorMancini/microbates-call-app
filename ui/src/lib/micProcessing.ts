/** Faixa de dBFS mostrada no medidor do microfone. */
export const NIVEL_MIN_DB = -60;
export const NIVEL_MAX_DB = 0;

/** Limiar do noise gate nas pontas da sensibilidade (99 → ~-60 dBFS, 0 → -20 dBFS). */
export const LIMIAR_MIN_DB = -60;
export const LIMIAR_MAX_DB = -20;

/**
 * Limiar do noise gate (dBFS) para a sensibilidade 0–100; `null` = gate aberto (100).
 * Espelho de `threshold_dbfs` em `crates/media/src/gate.rs`: mudar um exige mudar o outro.
 */
export function thresholdDbfs(sensibilidade: number): number | null {
  if (sensibilidade >= 100) return null;
  const limiar = LIMIAR_MIN_DB + (100 - sensibilidade) * 0.4;
  return Math.min(LIMIAR_MAX_DB, Math.max(LIMIAR_MIN_DB, limiar));
}

/** Posição (0–1) de um nível em dBFS na barra do medidor. */
export function nivelParaFracao(db: number): number {
  if (!Number.isFinite(db)) return 0;
  return Math.min(1, Math.max(0, (db - NIVEL_MIN_DB) / (NIVEL_MAX_DB - NIVEL_MIN_DB)));
}
