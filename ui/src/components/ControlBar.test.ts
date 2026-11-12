import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { createMockBackend } from '../lib/mock';
import { RoomStore } from '../lib/room.svelte';
import ControlBar from './ControlBar.svelte';

async function setup() {
  const store = new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
  await store.join({ serverUrl: 'ws://h', room: 'resenha', name: 'Ana' });
  render(ControlBar, { store });
  return { store, user: userEvent.setup() };
}

describe('ControlBar', () => {
  it('botão Mudo alterna aria-pressed e o estado do participante', async () => {
    const { store, user } = await setup();
    const mute = screen.getByRole('button', { name: 'Mudo' });
    expect(mute).toHaveAttribute('aria-pressed', 'false');
    await user.click(mute);
    expect(mute).toHaveAttribute('aria-pressed', 'true');
    expect(store.local?.muted).toBe(true);
    await user.click(mute);
    expect(mute).toHaveAttribute('aria-pressed', 'false');
  });

  it('Ensurdecer também marca Mudo', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Ensurdecer' }));
    expect(screen.getByRole('button', { name: 'Ensurdecer' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Mudo' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('compartilha tela pelo diálogo com fonte e qualidade', async () => {
    const { store, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Compartilhar tela' }));
    const dialog = await screen.findByRole('dialog', { name: 'Compartilhar tela' });
    expect(dialog).toBeVisible();
    await user.click(await screen.findByRole('radio', { name: /^Jogo/ }));
    await user.click(screen.getByRole('radio', { name: /720p/ }));
    await user.click(within(dialog).getByRole('button', { name: 'Compartilhar tela' }));
    expect(store.sharing).toBe(true);
    expect(store.sharePreset).toBe('720p30');
    await user.click(screen.getByRole('button', { name: 'Parar de compartilhar' }));
    expect(store.sharing).toBe(false);
  });

  it('seleciona dispositivo de saída', async () => {
    const { store, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Dispositivos' }));
    const output = await screen.findByRole('combobox', { name: 'Saída de áudio' });
    await screen.findByRole('option', { name: 'Headset' });
    await user.selectOptions(output, 'headset');
    expect(store.selectedOutput).toBe('headset');
  });

  it('Sair desconecta', async () => {
    const { store, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Sair' }));
    expect(store.status).toBe('disconnected');
  });
});