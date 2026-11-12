import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createMockBackend, type MockOptions } from './mock';
import {
  EVENTS,
  type AudioDevices,
  type Backend,
  type Participant,
  type RoomState,
  type ScreenSource,
  type ViewerCredentials,
} from './types';

/** Backend real: comandos e eventos exatamente como no CONTRACT.md. */
export function createTauriBackend(): Backend {
  const on =
    <T>(event: string) =>
    async (cb: (payload: T) => void) =>
      listen<T>(event, (e) => cb(e.payload));

  return {
    kind: 'tauri',
    joinRoom: ({ serverUrl, room, name, password }) =>
      invoke('join_room', { serverUrl, room, name, password: password ?? '' }),
    leaveRoom: () => invoke('leave_room'),
    setMuted: (muted) => invoke('set_muted', { muted }),
    setDeafened: (deafened) => invoke('set_deafened', { deafened }),
    listAudioDevices: () => invoke<AudioDevices>('list_audio_devices'),
    selectAudioDevice: (kind, id) => invoke('select_audio_device', { kind, id }),
    listScreenSources: () => invoke<ScreenSource[]>('list_screen_sources'),
    startScreenShare: (sourceId, preset) => invoke('start_screen_share', { sourceId, preset }),
    stopScreenShare: () => invoke('stop_screen_share'),
    onRoomState: on<RoomState>(EVENTS.state),
    onParticipants: on<Participant[]>(EVENTS.participants),
    openVideo: async () => {
      const creds = await invoke<ViewerCredentials>('viewer_credentials');
      const { connectVideoSession } = await import('./video');
      return connectVideoSession(creds);
    },
  };
}

/** Mock quando fora do Tauri (navegador puro) ou quando a URL pede `?mock=1`. */
export function shouldUseMock(search: string, insideTauri: boolean): boolean {
  const flag = new URLSearchParams(search).get('mock');
  if (flag === '1' || flag === 'true') return true;
  if (flag === '0' || flag === 'false') return !insideTauri;
  return !insideTauri;
}

export function mockOptionsFromSearch(search: string): MockOptions {
  const params = new URLSearchParams(search);
  return {
    remoteSharing: params.get('remoteShare') === '1',
    autoSpeak: params.get('speak') !== '0',
  };
}

export function createBackend(): Backend {
  const search = typeof location === 'undefined' ? '' : location.search;
  return shouldUseMock(search, isTauri())
    ? createMockBackend(mockOptionsFromSearch(search))
    : createTauriBackend();
}

/** Converte o erro do `invoke` (string serializada pelo Rust) em mensagem legível. */
export function errorMessage(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  return 'Algo deu errado. Tente de novo.';
}
