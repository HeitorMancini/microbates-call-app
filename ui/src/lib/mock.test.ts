import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from './mock';
import type { Participant, RoomState } from './types';

const join = { serverUrl: 'ws://26.1.2.3:7880', room: 'resenha', name: 'Ana' };

async function connected(opts = {}) {
  const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false, ...opts });
  const states: RoomState[] = [];
  const lists: Participant[][] = [];
  await backend.onRoomState((s) => states.push(s));
  await backend.onParticipants((p) => lists.push(p));
  await backend.joinRoom(join);
  return { backend, states, lists, local: () => lists.at(-1)!.find((p) => p.isLocal)! };
}

afterEach(() => vi.useRealTimers());

describe('backend mock', () => {
  it('entra na sala emitindo connecting -> connected e participantes', async () => {
    const { states, lists } = await connected();
    expect(states.map((s) => s.status)).toEqual(['connecting', 'connected']);
    const list = lists.at(-1)!;
    expect(list.length).toBeGreaterThan(1);
    expect(list.filter((p) => p.isLocal)).toEqual([expect.objectContaining({ name: 'Ana', muted: false })]);
  });

  it('rejeita entrada inválida com mensagem', async () => {
    const backend = createMockBackend({ latencyMs: 0, frames: false });
    await expect(backend.joinRoom({ ...join, name: '' })).rejects.toMatch(/nome/);
  });

  it('exige estar na sala para mutar', async () => {
    const backend = createMockBackend({ latencyMs: 0, frames: false });
    await expect(backend.setMuted(true)).rejects.toMatch(/não está/);
  });

  it('ensurdecer muta e restaura o estado anterior', async () => {
    const { backend, local } = await connected();
    await backend.setDeafened(true);
    expect(local().muted).toBe(true);
    await backend.setDeafened(false);
    expect(local().muted).toBe(false);
  });

  it('compartilhamento liga e desliga sharingScreen', async () => {
    const { backend, local } = await connected();
    const [source] = await backend.listScreenSources();
    await backend.startScreenShare(source.id, '720p30');
    expect(local().sharingScreen).toBe(true);
    await backend.stopScreenShare();
    expect(local().sharingScreen).toBe(false);
    await expect(backend.startScreenShare('nada', '720p30')).rejects.toMatch(/não encontrada/);
  });

  it('simula fala periodicamente e para ao sair', async () => {
    vi.useFakeTimers();
    let n = 0;
    const backend = createMockBackend({ latencyMs: 0, speakTickMs: 100, frames: false, random: () => (n++ % 2 ? 0 : 0.99) });
    const lists: Participant[][] = [];
    await backend.onParticipants((p) => lists.push(p));
    const joined = backend.joinRoom(join);
    await vi.advanceTimersByTimeAsync(0);
    await joined;
    await vi.advanceTimersByTimeAsync(250);
    expect(lists.some((l) => l.some((p) => p.speaking))).toBe(true);
    expect(lists.at(-1)!.every((p) => !(p.muted && p.speaking))).toBe(true);
    await backend.leaveRoom();
    const count = lists.length;
    await vi.advanceTimersByTimeAsync(500);
    expect(lists.length).toBe(count);
    expect(lists.at(-1)).toEqual([]);
  });

  it('unlisten para de entregar eventos', async () => {
    const backend = createMockBackend({ latencyMs: 0, frames: false });
    const cb = vi.fn();
    const off = await backend.onRoomState(cb);
    off();
    await backend.joinRoom(join);
    expect(cb).not.toHaveBeenCalled();
  });
});