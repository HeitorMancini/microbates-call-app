import { EventEmitter } from 'node:events';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Participant, RoomState } from './types';

class FakeParticipant extends EventEmitter {
  name = '';
  isMicrophoneEnabled = true;
  isSpeaking = false;
  isScreenShareEnabled = false;
  setMicrophoneEnabled = vi.fn(async (on: boolean) => {
    if (fake.micDenied) throw new Error('NotAllowedError');
    this.isMicrophoneEnabled = on;
  });
  setVolume = vi.fn();
  constructor(public identity: string) {
    super();
    this.name = identity;
  }
}

const fake = vi.hoisted(() => ({ room: undefined as unknown, micDenied: false }));

vi.mock('livekit-client', () => {
  class Room extends EventEmitter {
    localParticipant = new FakeParticipant('eu');
    remoteParticipants = new Map<string, FakeParticipant>();
    state = 'connected';
    connect = vi.fn(async () => {});
    disconnect = vi.fn(async () => {});
    startAudio = vi.fn(async () => {});
    getActiveDevice = vi.fn(() => undefined);
    constructor() {
      super();
      fake.room = this;
    }
    static getLocalDevices = vi.fn(async () => []);
  }
  return {
    Room,
    ConnectionState: { Connected: 'connected' },
    RoomEvent: {
      ParticipantConnected: 'participantConnected',
      ParticipantDisconnected: 'participantDisconnected',
      TrackMuted: 'trackMuted',
      TrackUnmuted: 'trackUnmuted',
      TrackPublished: 'trackPublished',
      TrackUnpublished: 'trackUnpublished',
      LocalTrackPublished: 'localTrackPublished',
      LocalTrackUnpublished: 'localTrackUnpublished',
      ActiveSpeakersChanged: 'activeSpeakersChanged',
      Reconnecting: 'reconnecting',
      Reconnected: 'reconnected',
      Disconnected: 'disconnected',
    },
    Track: { Source: { ScreenShare: 'screen_share' } },
  };
});

const { createWebBackend } = await import('./web');

const params = { serverUrl: 'wss://live.example.com:8443', room: 'Call 1', name: 'ana', password: 'segredo' };
const ok = () => new Response(JSON.stringify({ url: 'wss://lk', token: 'jwt' }), { status: 200 });

type FakeRoom = {
  localParticipant: FakeParticipant;
  remoteParticipants: Map<string, FakeParticipant>;
  connect: ReturnType<typeof vi.fn>;
  emit: (e: string, ...a: unknown[]) => boolean;
};
const room = () => fake.room as FakeRoom;

describe('WebBackend', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  beforeEach(() => {
    fetchMock = vi.fn(async () => ok());
    fake.micDenied = false;
  });

  it('pede o token ao servidor e conecta com a URL devolvida', async () => {
    const backend = createWebBackend(fetchMock as unknown as typeof fetch);
    await backend.joinRoom(params);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('https://live.example.com:8443/token');
    expect(JSON.parse(init.body as string)).toEqual({
      room: 'Call 1',
      name: 'ana',
      password: 'segredo',
      viewer: false,
    });
    expect(room().connect).toHaveBeenCalledWith('wss://lk', 'jwt');
  });

  it('senha recusada vira estado de erro com a mensagem do servidor', async () => {
    fetchMock.mockResolvedValue(new Response('wrong group password', { status: 401 }));
    const backend = createWebBackend(fetchMock as unknown as typeof fetch);
    const states: RoomState[] = [];
    await backend.onRoomState((s) => states.push(s));
    await expect(backend.joinRoom(params)).rejects.toThrow('wrong group password');
    expect(states.at(-1)).toEqual({ status: 'error', error: 'wrong group password' });
  });

  it('entra mesmo quando o microfone é negado e esconde os visualizadores', async () => {
    const backend = createWebBackend(fetchMock as unknown as typeof fetch);
    let list: Participant[] = [];
    await backend.onParticipants((p) => (list = p));
    const states: RoomState[] = [];
    await backend.onRoomState((s) => states.push(s));
    fake.micDenied = true;
    await backend.joinRoom(params);
    const bia = new FakeParticipant('bia');
    room().remoteParticipants.set('bia', bia);
    room().remoteParticipants.set('viewer~x', new FakeParticipant('viewer~x'));
    room().emit('participantConnected', bia);
    expect(states.at(-1)?.status).toBe('connected');
    expect(list.map((p) => p.id)).toEqual(['eu', 'bia']);
  });

  it('indicador de fala acompanha ActiveSpeakersChanged', async () => {
    const backend = createWebBackend(fetchMock as unknown as typeof fetch);
    let list: Participant[] = [];
    await backend.onParticipants((p) => (list = p));
    await backend.joinRoom(params);
    const bia = new FakeParticipant('bia');
    room().remoteParticipants.set('bia', bia);
    bia.isSpeaking = true;
    room().emit('activeSpeakersChanged');
    expect(list.find((p) => p.id === 'bia')?.speaking).toBe(true);
  });

  it('surdo zera o volume de todos e restaura o ajuste individual ao voltar', async () => {
    const backend = createWebBackend(fetchMock as unknown as typeof fetch);
    await backend.joinRoom(params);
    const bia = new FakeParticipant('bia');
    room().remoteParticipants.set('bia', bia);
    await backend.setParticipantVolume('bia', 150);
    expect(bia.setVolume).toHaveBeenLastCalledWith(1.5);
    await backend.setDeafened(true);
    expect(bia.setVolume).toHaveBeenLastCalledWith(0);
    await backend.setDeafened(false);
    expect(bia.setVolume).toHaveBeenLastCalledWith(1.5);
  });
});
