// Ilha React do voice-glow (MIT, Jakub Antalik): o brilho "dither" do botão.
// Carregada sob demanda para o React não entrar no pacote inicial.
import { createElement as h } from 'react';
import { createRoot } from 'react-dom/client';
import { VoiceBeam } from 'voice-glow';

export interface Brilho {
  atualizar(ativo: boolean, cores?: string[]): void;
  destruir(): void;
}

export function montarBrilho(alvo: HTMLElement): Brilho {
  const root = createRoot(alvo);
  const cheio = { display: 'block', width: '100%', height: '100%' };
  return {
    atualizar(ativo, cores) {
      root.render(
        h(VoiceBeam, {
          type: 'pill',
          look: 'dither',
          theme: 'dark',
          active: ativo,
          level: 0.6,
          idle: 0,
          pixelMagnet: 0,
          scale: 1.2,
          reach: 0.8,
          spread: 1.3,
          pixelSize: 1,
          colors: cores,
          style: cheio,
          children: h('span', { style: cheio }),
        }),
      );
    },
    destruir: () => root.unmount(),
  };
}
