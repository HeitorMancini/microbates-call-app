import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../lib/mock';
import type { Participant } from '../lib/types';
import Palco from './Palco.svelte';

// jsdom não tem ResizeObserver: reporta a janela de referência 1100×720.
class ResizeObserverFalso {
  #cb: ResizeObserverCallback;
  constructor(cb: ResizeObserverCallback) {
    this.#cb = cb;
  }
  observe() {
    this.#cb([{ contentRect: { width: 1100, height: 720 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
  }
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserverFalso);

const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false });

const p = (over: Partial<Participant>): Participant => ({
  id: over.name ?? 'x',
  name: 'x',
  muted: false,
  speaking: false,
  sharingScreen: false,
  isLocal: false,
  ...over,
});

describe('Palco', () => {
  it('lista as pessoas com rótulo e contagem', () => {
    render(Palco, { participants: [p({ name: 'Ana' }), p({ name: 'Bia' }), p({ name: 'Caio' })], backend });
    const list = screen.getByRole('list', { name: 'Pessoas na sala (3)' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(3);
  });

  it('sem ninguém mostra o aviso', () => {
    render(Palco, { participants: [], backend });
    expect(screen.getByText('Ninguém por aqui ainda. Chame a turma para a sala.')).toBeInTheDocument();
    expect(screen.queryByRole('list')).toBeNull();
  });

  it('posiciona cada blob pela roda', () => {
    render(Palco, { participants: [p({ name: 'Ana' }), p({ name: 'Bia' })], backend });
    const list = screen.getByRole('list', { name: 'Pessoas na sala (2)' });
    const [first] = within(list).getAllByRole('listitem');
    // Centrado no ponto (430) independente da largura real do item; 346 − 48
    // (metade do avatar grande): roda sem tela, 2 pessoas, janela 1100×720.
    expect(first).toHaveAttribute('style', expect.stringContaining('translate(calc(430px - 50%), 298px)'));
  });

  it('alguém compartilha → modo palco; Roda → tela pequena; clicar na tela → palco', async () => {
    const user = userEvent.setup();
    const { rerender } = render(Palco, {
      participants: [p({ name: 'Ana', isLocal: true }), p({ name: 'Bia' }), p({ name: 'Caio' })],
      backend,
    });

    await rerender({
      participants: [p({ name: 'Ana', isLocal: true }), p({ name: 'Bia', sharingScreen: true }), p({ name: 'Caio' })],
      backend,
    });

    const tela = screen.getByTestId('palco-tela');
    expect(tela).not.toHaveClass('pequena');
    const blobs = screen.getAllByTestId('participante');
    expect(blobs.every((b) => b.getAttribute('data-pequeno') === 'true')).toBe(true);

    await user.click(screen.getByRole('button', { name: 'Roda' }));
    expect(tela).toHaveClass('pequena');
    expect(screen.getAllByTestId('participante').every((b) => b.getAttribute('data-pequeno') === 'false')).toBe(true);

    await user.click(tela);
    expect(tela).not.toHaveClass('pequena');
  });

  it('roda → "Ampliar tela" alcançável por teclado volta ao palco', async () => {
    const user = userEvent.setup();
    render(Palco, {
      participants: [p({ name: 'Ana', isLocal: true }), p({ name: 'Bia', sharingScreen: true }), p({ name: 'Caio' })],
      backend,
    });

    await user.click(screen.getByRole('button', { name: 'Roda' }));
    const tela = screen.getByTestId('palco-tela');
    expect(tela).toHaveClass('pequena');

    const ampliar = screen.getByRole('button', { name: 'Ampliar tela' });
    ampliar.focus();
    await user.keyboard('{Enter}');
    expect(tela).not.toHaveClass('pequena');
    expect(screen.queryByRole('button', { name: 'Ampliar tela' })).toBeNull();
  });

  it('clicar no AO VIVO troca a tela assistida', async () => {
    const user = userEvent.setup();
    render(Palco, {
      participants: [
        p({ name: 'Ana', isLocal: true }),
        p({ name: 'Bia', sharingScreen: true }),
        p({ name: 'Caio', sharingScreen: true }),
      ],
      backend,
    });

    expect(screen.getByRole('heading', { name: 'Tela de Bia' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'AO VIVO — assistir a tela de Caio' }));
    expect(screen.getByRole('heading', { name: 'Tela de Caio' })).toBeInTheDocument();
  });

  it('quem você assiste para → cai para quem ainda compartilha; último para → roda sem tela; novo compartilhamento abre no palco', async () => {
    const { rerender } = render(Palco, {
      participants: [
        p({ name: 'Ana', isLocal: true }),
        p({ name: 'Bia', sharingScreen: true }),
        p({ name: 'Caio', sharingScreen: true }),
        p({ name: 'Gui' }),
      ],
      backend,
    });

    // Assiste a quem compartilha por padrão (primeiro da lista, Bia); evita
    // clicar em AO VIVO aqui, que por si só já reabre a sessão de vídeo ao
    // trocar de pessoa assistida (comportamento à parte deste achado).
    expect(screen.getByRole('heading', { name: 'Tela de Bia' })).toBeInTheDocument();
    const videoAntes = screen.getByTestId('screen-video');

    await rerender({
      participants: [
        p({ name: 'Ana', isLocal: true }),
        p({ name: 'Bia' }),
        p({ name: 'Caio', sharingScreen: true }),
        p({ name: 'Gui' }),
      ],
      backend,
    });
    expect(screen.getByRole('heading', { name: 'Tela de Caio' })).toBeInTheDocument();
    // `sharer` não pode passar por `null` quando a pessoa assistida (Bia) para
    // de compartilhar: se passasse, o ScreenViewer seria desmontado e
    // remontado (o <video> seria recriado), reabrindo o vídeo mesmo sem o
    // usuário ter trocado de tela.
    expect(screen.getByTestId('screen-video')).toBe(videoAntes);

    await rerender({
      participants: [p({ name: 'Ana', isLocal: true }), p({ name: 'Bia' }), p({ name: 'Caio' }), p({ name: 'Gui' })],
      backend,
    });
    expect(screen.queryByTestId('palco-tela')).toBeNull();

    await rerender({
      participants: [
        p({ name: 'Ana', isLocal: true }),
        p({ name: 'Bia' }),
        p({ name: 'Caio' }),
        p({ name: 'Gui', sharingScreen: true }),
      ],
      backend,
    });
    expect(screen.getByTestId('palco-tela')).not.toHaveClass('pequena');
  });

  it('botão direito abre o ajuste de volume e clicar no escuro fecha', async () => {
    const onvolume = vi.fn();
    render(Palco, {
      participants: [p({ name: 'Ana', isLocal: true }), p({ name: 'Bia' })],
      backend,
      volumeOf: () => 73,
      onvolume,
    });
    const bia = screen.getAllByTestId('participante')[1];
    await userEvent.pointer({ keys: '[MouseRight]', target: bia });
    const grupo = screen.getByRole('group', { name: /Volume de Bia/ });
    expect((within(grupo).getByRole('slider') as HTMLInputElement).value).toBe('73');
    await userEvent.pointer({ keys: '[MouseLeft]', target: document.querySelector('.escuro')! });
    await vi.waitFor(() => expect(screen.queryByRole('group', { name: /Volume de Bia/ })).toBeNull());
  });
});