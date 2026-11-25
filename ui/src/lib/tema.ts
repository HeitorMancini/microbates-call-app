import type { Destaque } from './cor';

const NEUTRO = { cor: '#6a6a72', texto: '#ececf0' };

/** Aplica a cor de destaque do usuário (ou o neutro) nos tokens do app inteiro. */
export function aplicarDestaque(d: Pick<Destaque, 'cor' | 'texto'> | null) {
  const { cor, texto } = d ?? NEUTRO;
  const raiz = document.documentElement.style;
  raiz.setProperty('--color-valvula', cor);
  raiz.setProperty('--color-valvula-texto', texto);
  raiz.setProperty('--color-valvula-fundo', `color-mix(in srgb, ${cor} 22%, #0b0b0d)`);
}
