import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
const listen = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), isTauri: () => false }));
vi.mock('@tauri-apps/api/event', () => ({ listen: (...a: unknown[]) => listen(...a) }));

const { createTauriBackend, createBackend, errorMessage, shouldUseMock, shouldUseWeb } = await import('./api');

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

describe('shouldUseWeb', () => {
  it('só no build web, fora do Tauri e sem ?mock', () => {
    expect(shouldUseWeb('', false, 'web')).toBe(true);
    expect(shouldUseWeb('?mock=1', false, 'web')).toBe(false);
    expect(shouldUseWeb('', true, 'web')).toBe(false);
    expect(shouldUseWeb('', false, 'development')).toBe(false);
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
    await b.setMicVolume(120);
    await b.setNoiseSuppression(false);
    await b.setMicSensitivity(40);
    await b.setParticipantVolume('ana', 60);
    await b.startScreenShare('monitor:0',     '1080p60', true);
    await b.stopScreenShare();
    expect(invoke.mock.calls).toEqual([
      ['join_room', { serverUrl: 'ws://h', room: 's', name: 'n', password: '' }],
      ['leave_room'],
      ['set_muted', { muted: true }],
      ['set_deafened', { deafened: false }],
      ['select_audio_device', { kind: 'output', id: 'headset' }],
      ['set_mic_volume', { percent: 120 }],
      ['set_noise_suppression', { enabled: false }],
      ['set_mic_sensitivity', { percent: 40 }],
      ['set_participant_volume', { id: 'ana', percent: 60 }],
      ['start_screen_share', { sourceId: 'monitor:0', preset: '1080p60', withAudio: true }],
      ['stop_screen_share'],
    ]);
  });

  it('getMicLevel devolve o nível em dBFS do comando mic_level', async () => {
    invoke.mockResolvedValueOnce(-37.5);
    await expect(createTauriBackend().getMicLevel()).resolves.toBe(-37.5);
    expect(invoke).toHaveBeenCalledWith('mic_level');
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