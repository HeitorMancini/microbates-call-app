// Envelope de voz simulado: o backend só manda "falando: sim/não", sem nível.
// Cada chamador recebe seu próprio envelope, com alvos aleatórios a cada
// 90–220 ms (20% de chance de pausa) interpolados em ~60 ms, para o brilho
// parecer uma fala de verdade em vez de um liga/desliga seco.

const PAUSA = 0.05;
const CHANCE_PAUSA = 0.2;
const INTERVALO_MIN_MS = 90;
const INTERVALO_MAX_MS = 220;
const INTERPOLACAO_MS = 60;

/** Cria um gerador de nível de voz simulado em [0.05, 1] a partir do tempo em ms. */
export function criarEnvelope(aleatorio: () => number = Math.random): (t: number) => number {
  let proximoAlvoEm = -Infinity;
  let alvoAnterior = PAUSA;
  let alvoAtual = PAUSA;
  let inicioInterpolacao = 0;

  function sortearAlvo(): number {
    return aleatorio() < CHANCE_PAUSA ? PAUSA : 0.45 + aleatorio() * 0.55;
  }

  return (t: number): number => {
    if (t >= proximoAlvoEm) {
      alvoAnterior = alvoAtual;
      alvoAtual = sortearAlvo();
      inicioInterpolacao = t;
      proximoAlvoEm = t + INTERVALO_MIN_MS + aleatorio() * (INTERVALO_MAX_MS - INTERVALO_MIN_MS);
    }
    const progresso = Math.min(1, (t - inicioInterpolacao) / INTERPOLACAO_MS);
    const nivel = alvoAnterior + (alvoAtual - alvoAnterior) * progresso;
    return Math.min(1, Math.max(PAUSA, nivel));
  };
}
