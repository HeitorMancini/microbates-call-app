import {
  ConnectionState,
  Room,
  RoomEvent,
  type Participant as LkParticipant,
  type RemoteParticipant,
} from 'livekit-client';
import { tokenEndpoint } from './servidor';
import type {
  AudioDevices,
  Backend,
  Device,
  DeviceKind,
  JoinParams,
  Participant,
  RoomState,
  VideoSession,
} from './types';
import { videoSessionFromRoom } from './video';

const VIEWER_PREFIX = 'viewer~';

const REFRESH_EVENTS = [
  RoomEvent.ParticipantConnected,
  RoomEvent.ParticipantDisconnected,
  RoomEvent.TrackMuted,
  RoomEvent.TrackUnmuted,
  RoomEvent.TrackPublished,
  RoomEvent.TrackUnpublished,
  RoomEvent.LocalTrackPublished,
  RoomEvent.LocalTrackUnpublished,
  RoomEvent.ActiveSpeakersChanged,
] as const;

const KIND: Record<DeviceKind, MediaDeviceKind> = { input: 'audioinput', output: 'audiooutput' };

async function requestCredentials(
  fetchImpl: typeof fetch,
  { serverUrl, room, name, password }: JoinParams,
): Promise<{ url: string; token: string }> {
  const res = await fetchImpl(tokenEndpoint(serverUrl), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ room, name, password: password ?? '', viewer: false }),
  });
  if (!res.ok) {
    const text = (await res.text().catch(() => '')).trim();
    throw new Error(text || `O servidor recusou a entrada (${res.status}).`);
  }
  return (await res.json()) as { url: string; token: string };
}

function describe(p: LkParticipant, isLocal: boolean): Participant {
  return {
    id: p.identity,
    name: p.name || p.identity,
    muted: !p.isMicrophoneEnabled,
    speaking: p.isSpeaking,
    sharingScreen: p.isScreenShareEnabled,
    isLocal,
  };
}

/** Backend do navegador (PWA): conecta direto ao LiveKit, sem o motor nativo do desktop. */
export function createWebBackend(fetchImpl: typeof fetch = (...a) => fetch(...a)): Backend {
  let room: Room | undefined;
  let deafened = false;
  const volumes = new Map<string, number>();
  const stateCbs = new Set<(s: RoomState) => void>();
  const peopleCbs = new Set<(p: Participant[]) => void>();

  const emitState = (state: RoomState) => stateCbs.forEach((cb) => cb(state));

  const people = (): Participant[] => {
    if (!room) return [];
    const remotes = [...room.remoteParticipants.values()].filter((p) => !p.identity.startsWith(VIEWER_PREFIX));
    return [describe(room.localParticipant, true), ...remotes.map((p) => describe(p, false))];
  };
  const emitPeople = () => {
    const list = people();
    peopleCbs.forEach((cb) => cb(list));
  };

  const applyVolume = (p: RemoteParticipant) =>
    p.setVolume(deafened ? 0 : (volumes.get(p.identity) ?? 100) / 100);
  const applyVolumes = () => room?.remoteParticipants.forEach(applyVolume);

  const unsupported = (what: string) => () => Promise.reject(new Error(`${what} não está disponível no navegador.`));
  const subscribe =
    <T>(set: Set<(v: T) => void>) =>
    async (cb: (v: T) => void) => {
      set.add(cb);
      return () => {
        set.delete(cb);
      };
    };

  return {
    kind: 'web',
    async joinRoom(params) {
      emitState({ status: 'connecting' });
      try {
        const creds = await requestCredentials(fetchImpl, params);
        const next = new Room({ adaptiveStream: true, dynacast: true });
        REFRESH_EVENTS.forEach((e) => next.on(e, emitPeople));
        next.on(RoomEvent.ParticipantConnected, applyVolume);
        next.on(RoomEvent.Reconnecting, () => emitState({ status: 'reconnecting' }));
        next.on(RoomEvent.Reconnected, () => emitState({ status: 'connected' }));
        next.on(RoomEvent.Disconnected, () => {
          if (room === next) emitState({ status: 'disconnected' });
        });
        await next.connect(creds.url, creds.token);
        room = next;
        // Ouvir exige gesto do usuário (autoplay): o clique em "Entrar" chega até aqui.
        await next.startAudio().catch(() => {});
        // Sem permissão de microfone a pessoa entra só ouvindo.
        await next.localParticipant.setMicrophoneEnabled(true).catch(() => {});
        applyVolumes();
        emitState({ status: 'connected' });
        emitPeople();
      } catch (err) {
        emitState({ status: 'error', error: err instanceof Error ? err.message : String(err) });
        throw err;
      }
    },
    async leaveRoom() {
      const current = room;
      room = undefined;
      await current?.disconnect();
      emitState({ status: 'disconnected' });
      peopleCbs.forEach((cb) => cb([]));
    },
    async setMuted(muted) {
      await room?.localParticipant.setMicrophoneEnabled(!muted);
      emitPeople();
    },
    async setDeafened(value) {
      deafened = value;
      applyVolumes();
    },
    async listAudioDevices(): Promise<AudioDevices> {
      const toDevices = (list: MediaDeviceInfo[]): Device[] =>
        list.map((d) => ({ id: d.deviceId, name: d.label || 'Dispositivo' }));
      const [inputs, outputs] = await Promise.all([
        Room.getLocalDevices('audioinput').catch(() => []),
        Room.getLocalDevices('audiooutput').catch(() => []),
      ]);
      return {
        inputs: toDevices(inputs),
        outputs: toDevices(outputs),
        currentInput: room?.getActiveDevice('audioinput') ?? null,
        currentOutput: room?.getActiveDevice('audiooutput') ?? null,
      };
    },
    async selectAudioDevice(kind, id) {
      await room?.switchActiveDevice(KIND[kind], id);
    },
    async setMicVolume() {},
    setNoiseSuppression: unsupported('Supressão de ruído'),
    setMicSensitivity: unsupported('Sensibilidade do microfone'),
    getMicLevel: async () => -100,
    async setParticipantVolume(id, percent) {
      volumes.set(id, percent);
      const p = room?.remoteParticipants.get(id);
      if (p) applyVolume(p);
    },
    listScreenSources: async () => [],
    startScreenShare: unsupported('Compartilhar tela'),
    stopScreenShare: async () => {},
    onRoomState: subscribe(stateCbs),
    onParticipants: subscribe(peopleCbs),
    getUpdateState: async () => ({ status: 'idle' }),
    onUpdateState: async () => () => {},
    installUpdate: async () => {},
    checkForUpdate: async () => false,
    getAppVersion: async () => __APP_VERSION__,
    async openVideo(): Promise<VideoSession> {
      if (!room || room.state !== ConnectionState.Connected) throw new Error('Entre numa call primeiro.');
      const current = room;
      const subscribeScreens = (p: RemoteParticipant) =>
        p.trackPublications.forEach((pub) => pub.source === 'screen_share' && pub.setSubscribed(true));
      current.remoteParticipants.forEach(subscribeScreens);
      current.on(RoomEvent.ParticipantConnected, subscribeScreens);
      current.on(RoomEvent.TrackPublished, (pub) => pub.source === 'screen_share' && pub.setSubscribed(true));
      return videoSessionFromRoom(current, false);
    },
  };
}
