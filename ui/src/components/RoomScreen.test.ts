import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../lib/mock';
import { RoomStore } from '../lib/room.svelte';
import RoomScreen from './RoomScreen.svelte';

// jsdom não tem ResizeObserver: o Palco (dentro do RoomScreen) usa a janela de referência 1100×720.
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

async function setup() {
  const store = new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
  await store.join({ serverUrl: 'ws://h', room: 'Call 1', name: 'Ana' });
  render(RoomScreen, { store });
  return { store, user: userEvent.setup() };
}

describe('RoomScreen', () => {
  it('título é a sala atual', async () => {
    await setup();
    expect(screen.getByRole('heading', { level: 1, name: 'Call 1' })).toBeInTheDocument();
  });

  it('Configurações abre Dispositivos de áudio', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Configurações' }));
    expect(await screen.findByRole('dialog', { name: 'Dispositivos de áudio' })).toBeVisible();
  });

  it('Sair da call desconecta', async () => {
    const { store, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Sair da call' }));
    expect(store.status).toBe('disconnected');
  });

  it('trocar no seletor muda de sala', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Call 1' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Call 2' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'Call 2' })).toBeInTheDocument();
  });

  it('chat está marcado como em breve', async () => {
    await setup();
    const chat = screen.getByRole('button', { name: 'Chat de texto (em breve)' });
    expect(chat).toHaveAttribute('aria-disabled', 'true');
    expect(chat).toHaveAttribute('title', 'Chat de texto · em breve');
  });

  it('mostra status só quando não está conectado', async () => {
    await setup();
    expect(screen.queryByText('Reconectando…')).toBeNull();
  });
});
