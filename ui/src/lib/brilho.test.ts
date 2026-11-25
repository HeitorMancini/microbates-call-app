import { describe, expect, it, vi } from 'vitest';

const render = vi.fn();
const unmount = vi.fn();
vi.mock('react-dom/client', () => ({ createRoot: () => ({ render, unmount }) }));
vi.mock('voice-glow', () => ({ VoiceBeam: 'voice-beam' }));

const { montarBrilho } = await import('./brilho');

describe('montarBrilho', () => {
  it('passa a função nivel ao VoiceBeam (lida a cada quadro, não uma vez por atualizar)', () => {
    const nivel = () => 0.42;
    const brilho = montarBrilho(document.createElement('span'), { nivel });
    brilho.atualizar(true, undefined);
    const vnode = render.mock.calls[0][0] as { props: { level: unknown } };
    expect(vnode.props.level).toBe(nivel);
  });

  it('sem nivel explícito usa 0.6 fixo (botão de login)', () => {
    const brilho = montarBrilho(document.createElement('span'));
    brilho.atualizar(true, undefined);
    const vnode = render.mock.calls[0][0] as { props: { level: () => number } };
    expect(typeof vnode.props.level).toBe('function');
    expect(vnode.props.level()).toBe(0.6);
  });
});
