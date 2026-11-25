import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../lib/mock';
import { RoomStore } from '../lib/room.svelte';
import JoinScreen from './JoinScreen.svelte';

function setup() {
  const store = new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
  const join = vi.spyOn(store, 'join');
  render(JoinScreen, { store });
  return { store, join, user: userEvent.setup() };
}

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('style');
});

describe('JoinScreen', () => {
  it('pede só nome e senha; servidor e sala ficam ocultos', () => {
    setup();
    expect(screen.getByLabelText('Nome')).toBeInTheDocument();
    expect(screen.getByLabelText('Senha da sala')).toHaveAttribute('type', 'password');
    expect(screen.queryByLabelText('Servidor')).toBeNull();
    expect(screen.queryByLabelText('Sala')).toBeNull();
  });

  it('sem nome, mostra o erro e foca o campo', async () => {
    const { store, join, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Entrar' }));
    const nome = screen.getByLabelText('Nome');
    expect(nome).toHaveAttribute('aria-invalid', 'true');
    expect(nome).toHaveAccessibleDescription(/nome/i);
    expect(nome).toHaveFocus();
    expect(join).not.toHaveBeenCalled();
    expect(store.status).toBe('disconnected');
  });

  it('entra no servidor e na sala fixos, aplica a cor do blob e lembra nome e senha', async () => {
    const { store, join, user } = setup();
    await user.type(screen.getByLabelText('Nome'), 'Ana');
    await user.type(screen.getByLabelText('Senha da sala'), 'segredo');
    await user.click(screen.getByRole('button', { name: 'Entrar' }));

    expect(join).toHaveBeenCalledWith({ serverUrl: 'wss://live.microbates.com:8443', room: 'Call 1', name: 'Ana', password: 'segredo' });
    await vi.waitFor(() => expect(store.status).toBe('connected'));
    expect(document.documentElement.style.getPropertyValue('--color-valvula')).toMatch(/^#[0-9a-f]{6}$/i);
    expect(JSON.parse(localStorage.getItem('sheeshcord:entrada')!)).toEqual({ name: 'Ana', password: 'segredo' });
  });

  it('mostra o erro do servidor quando a senha está errada', async () => {
    const { store, user } = setup();
    await user.type(screen.getByLabelText('Nome'), 'Ana');
    await user.type(screen.getByLabelText('Senha da sala'), 'errada');
    await user.click(screen.getByRole('button', { name: 'Entrar' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/senha do grupo incorreta/);
    expect(store.status).not.toBe('connected');
  });

  it('volta para a cor neutra enquanto o nome é editado', async () => {
    localStorage.setItem('sheeshcord:entrada', JSON.stringify({ name: 'Ana', password: '' }));
    const { user } = setup();
    const raiz = document.documentElement.style;
    expect(raiz.getPropertyValue('--color-valvula')).not.toBe('#6a6a72');
    await user.type(screen.getByLabelText('Nome'), 'x');
    expect(raiz.getPropertyValue('--color-valvula')).toBe('#6a6a72');
  });
});
