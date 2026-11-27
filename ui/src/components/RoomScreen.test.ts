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

async function setup(kind?: 'web') {
  const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false });
  if (kind) Object.defineProperty(backend, 'kind', { value: kind });
  const store = new RoomStore(backend);
  await store.join({ serverUrl: 'ws://h', room: 'Call 1', name: 'Ana' });
  render(RoomScreen, { store });
  return { store, user: userEvent.setup() };
}

describe('RoomScreen', () => {
  it('título é a sala atual', async () => {
    await setup();
    expect(screen.getByRole('heading', { level: 1, name: 'Call 1' })).toBeInTheDocument();
  });

  it('tem os marcos main (palco) e footer (controles)', async () => {
    await setup();
    expect(screen.getByRole('main', { name: 'Sala Call 1' })).toBeInTheDocument();
    expect(screen.getByRole('contentinfo')).toBeInTheDocument();
  });

  it('Configurações abre Configurações', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Configurações' }));
    expect(await screen.findByRole('dialog', { name: 'Configurações' })).toBeVisible();
  });

  it('Configurações → mostra a versão e verifica atualizações', async () => {
    const { store, user } = await setup();
    vi.spyOn(store.backend, 'getAppVersion').mockResolvedValue('1.2.3');
    await user.click(screen.getByRole('button', { name: 'Configurações' }));
    expect(await screen.findByText('Versão 1.2.3')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Verificar atualizações' }));
    expect(await screen.findByText('Você já está na versão mais recente.')).toBeVisible();
  });

  it('no navegador some o que só existe no desktop', async () => {
    const { user } = await setup('web');
    await user.click(screen.getByRole('button', { name: 'Configurações' }));
    await screen.findByRole('dialog', { name: 'Configurações' });
    expect(screen.queryByRole('button', { name: 'Verificar atualizações' })).toBeNull();
    expect(screen.queryByLabelText('Supressão de ruído')).toBeNull();
    expect(screen.queryByRole('button', { name: /Compartilhar tela/ })).toBeNull();
  });

  it('Configurações → seleciona dispositivo de saída', async () => {
    const { store, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Configurações' }));
    await screen.findByRole('dialog', { name: 'Configurações' });
    const output = await screen.findByRole('combobox', { name: 'Saída de áudio' });
    await screen.findByRole('option', { name: 'Headset' });
    await user.selectOptions(output, 'headset');
    expect(store.selectedOutput).toBe('headset');
  });

  it('atualização pronta: indicador pede confirmação antes de reiniciar', async () => {
    const { store, user } = await setup();
    const install = vi.spyOn(store.backend, 'installUpdate');
    expect(screen.queryByRole('button', { name: /Reiniciar/ })).toBeNull();

    store.update = { status: 'downloading', percent: 40 };
    expect(await screen.findByRole('status')).toHaveTextContent('Baixando atualização 40%');

    store.update = { status: 'ready', version: '0.6.0' };
    await user.click(await screen.findByRole('button', { name: 'Reiniciar para atualizar (v0.6.0)' }));
    expect(install).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Isso encerra a call. Reiniciar?' }));
    expect(install).toHaveBeenCalledOnce();
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
