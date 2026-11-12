import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
const listen = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), isTauri: () => false }));
vi.mock('@tauri-apps/api/event', () => ({ listen: (...a: unknown[]) => listen(...a) }));

const { createTauriBackend, createBackend, errorMessage, shouldUseMock } = await import('./api');

beforeEach(() => {
  invoke.mockReset().mockResolvedValue(undefined);
  listen.mockReset().mockResolvedValue(() => {});
});

describe('shouldUseMock', () => {
  it('usa mock fora do Tauri ou com ?mock=1', () => {
    expect(shouldUseMock('', false)).toBe(true);
    expect(shouldUseMock('', true)).toBe(false);
    expect(shouldUseMock('?mock=1', true)).toBe(true);
    expect(shouldUseMock('?mock=0', true)).toBe(false);
  });

  it('createBackend cai no mock no navegador', () => {
    expect(createBackend().kind).toBe('mock');
  });
});

describe('backend Tauri segue o CONTRACT.md', () => {
  it('mapeia comandos e argumentos', async () => {
    const b = createTauriBackend();
    await b.joinRoom({ serverUrl: 'ws://h', room: 's', name: 'n' });
    await b.leaveRoom();
    await b.setMuted(true);
    await b.setDeafened(false);
    await b.selectAudioDevice('output', 'headset');
    await b.startScreenShare('monitor:0', '1080p60');
    await b.stopScreenShare();
    expect(invoke.mock.calls).toEqual([
      ['join_room', { serverUrl: 'ws://h', room: 's', name: 'n', password: '' }],
      ['leave_room'],
      ['set_muted', { muted: true }],
      ['set_deafened', { deafened: false }],
      ['select_audio_device', { kind: 'output', id: 'headset' }],
      ['start_screen_share', { sourceId: 'monitor:0', preset: '1080p60' }],
      ['stop_screen_share'],
    ]);
  });

  it('repassa o payload dos eventos room://*', async () => {
    const b = createTauriBackend();
    const cb = vi.fn();
    await b.onParticipants(cb);
    expect(listen).toHaveBeenCalledWith('room://participants', expect.any(Function));
    const handler = listen.mock.calls[0][1] as (e: { payload: unknown }) => void;
    handler({ payload: [{ id: 'a' }] });
    expect(cb).toHaveBeenCalledWith([{ id: 'a' }]);
    await b.onRoomState(cb);
    expect(listen.mock.calls.map((c) => c[0])).toEqual(['room://participants', 'room://state']);
  });

  it('abre o vídeo com credenciais somente-assinatura vindas do Rust', async () => {
    invoke.mockRejectedValueOnce('Você não está em nenhuma sala.');
    await expect(createTauriBackend().openVideo()).rejects.toMatch(/nenhuma sala/);
    expect(invoke).toHaveBeenCalledWith('viewer_credentials');
  });
});

describe('errorMessage', () => {
  it('aceita string do Rust, Error e desconhecido', () => {
    expect(errorMessage('Você não está em nenhuma sala.')).toBe('Você não está em nenhuma sala.');
    expect(errorMessage(new Error('x'))).toBe('x');
    expect(errorMessage(42)).toMatch(/errado/);
  });
});