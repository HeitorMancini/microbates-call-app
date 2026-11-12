import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../lib/mock';
import { RoomStore } from '../lib/room.svelte';
import JoinScreen from './JoinScreen.svelte';

function setup() {
  const store = new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
  render(JoinScreen, { store });
  return { store, user: userEvent.setup() };
}

beforeEach(() => localStorage.clear());

describe('JoinScreen', () => {
  it('mostra erros acessíveis e foca o primeiro campo inválido', async () => {
    const { store, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Entrar na sala' }));

    const server = screen.getByLabelText('Servidor');
    expect(server).toHaveAttribute('aria-invalid', 'true');
    expect(server).toHaveAccessibleDescription(/Informe o endereço do servidor/);
    expect(screen.getByLabelText('Sala')).toHaveAccessibleDescription('Informe o nome da sala.');
    expect(screen.getByLabelText('Seu nome')).toHaveAttribute('aria-invalid', 'true');
    expect(server).toHaveFocus();
    expect(store.status).toBe('disconnected');
  });

  it('revalida enquanto a pessoa corrige', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Entrar na sala' }));
    await user.type(screen.getByLabelText('Sala'), 'resenha');
    expect(screen.getByLabelText('Sala')).not.toHaveAttribute('aria-invalid');
  });

  it('rejeita endereço sem protocolo', async () => {
    const { user } = setup();
    await user.type(screen.getByLabelText('Servidor'), '26.1.2.3:7880');
    await user.click(screen.getByRole('button', { name: 'Entrar na sala' }));
    expect(screen.getByLabelText('Servidor')).toHaveAccessibleDescription(/ws:\/\/26\.1\.2\.3:7880/);
  });

  it('entra com dados válidos e lembra deles', async () => {
    const { store, user } = setup();
    await user.type(screen.getByLabelText('Servidor'), 'ws://26.1.2.3:7880');
    await user.type(screen.getByLabelText('Sala'), 'resenha');
    await user.type(screen.getByLabelText('Seu nome'), 'Ana');
    await user.click(screen.getByRole('button', { name: 'Entrar na sala' }));
    await vi.waitFor(() => expect(store.status).toBe('connected'));
    expect(JSON.parse(localStorage.getItem('sheeshcord:entrada')!)).toMatchObject({ room: 'resenha', name: 'Ana' });
  });
});