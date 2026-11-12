import { hasErrors, normalizeJoin, validateJoin } from './validation';
import type {
  AudioDevices,
  Backend,
  DeviceKind,
  JoinParams,
  Participant,
  RoomState,
  ScreenPreset,
  ScreenSource,
  Unlisten,
  VideoSession,
  VideoUpdate,
} from './types';

export interface MockOptions {
  /** Atraso simulado entre `connecting` e `connected`. */
  latencyMs?: number;
  /** Intervalo entre mudanças de quem está falando (0 desliga). */
  speakTickMs?: number;
  autoSpeak?: boolean;
  /** Gera vídeo de teste (canvas.captureStream) para quem compartilha tela. */
  frames?: boolean;
  /** Um amigo já entra compartilhando tela. */
  remoteSharing?: boolean;
  random?: () => number;
}

const FRIENDS = ['Bia Rocha', 'Caio', 'Duda Lima', 'Gui'];

/** Senha que o mock recusa, para simular o 401 do token-server. */
export const MOCK_WRONG_PASSWORD = 'errada';

const DEVICES: AudioDevices = {
  inputs: [
    { id: 'default', name: 'Padrão do sistema' },
    { id: 'mic-usb', name: 'Microfone USB' },
  ],
  outputs: [
    { id: 'default', name: 'Padrão do sistema' },
    { id: 'headset', name: 'Headset' },
  ],
};

const SOURCES: ScreenSource[] = [
  { id: 'monitor:0', name: 'Tela principal', kind: 'monitor' },
  { id: 'monitor:1', name: 'Segundo monitor', kind: 'monitor' },
  { id: 'window:game', name: 'Jogo', kind: 'window' },
];

const PRESET_SIZE: Record<ScreenPreset, [number, number, number]> = {
  '1080p30': [1920, 1080, 30],
  '1080p60': [1920, 1080, 60],
  '720p30': [1280, 720, 30],
};

type Handlers = {
  state: Set<(s: RoomState) => void>;
  participants: Set<(p: Participant[]) => void>;
};

const canMakeVideo = () =>
  typeof document !== 'undefined' &&
  typeof navigator !== 'undefined' &&
  !/jsdom/i.test(navigator.userAgent);

/** Backend falso para desenvolvimento no navegador e E2E. */
export function createMockBackend(options: MockOptions = {}): Backend & { readonly participants: Participant[] } {
  const {
    latencyMs = 350,
    speakTickMs = 700,
    autoSpeak = true,
    frames = canMakeVideo(),
    remoteSharing = false,
    random = Math.random,
  } = options;

  const handlers: Handlers = { state: new Set(), participants: new Set() };
  let participants: Participant[] = [];
  let connected = false;
  let deafened = false;
  let mutedBeforeDeafen = false;
  let share: { preset: ScreenPreset } | null = null;
  let speakTimer: ReturnType<typeof setInterval> | undefined;
  let joinTimer: ReturnType<typeof setTimeout> | undefined;

  const emitState = (s: RoomState) => handlers.state.forEach((cb) => cb({ ...s }));
  const emitParticipants = () => {
    const snapshot = participants.map((p) => ({ ...p }));
    handlers.participants.forEach((cb) => cb(snapshot));
  };
  const local = () => participants.find((p) => p.isLocal);
  const requireLocal = () => {
    const me = local();
    if (!connected || !me) throw 'Você não está em nenhuma sala.';
    return me;
  };
  const update = (id: string, patch: Partial<Participant>) => {
    participants = participants.map((p) => (p.id === id ? { ...p, ...patch } : p));
  };

  function tickSpeaking() {
    let changed = false;
    for (const p of participants) {
      if (p.muted) continue;
      const speaking = random() < (p.isLocal ? 0.25 : 0.4);
      if (speaking !== p.speaking) {
        update(p.id, { speaking });
        changed = true;
      }
    }
    if (changed) emitParticipants();
  }

  function stopTimers() {
    clearInterval(speakTimer);
    clearTimeout(joinTimer);
    speakTimer = joinTimer = undefined;
  }

  function videoSession(): VideoSession {
    return {
      watch(id, el, onUpdate) {
        let frameNo = 0;
        let timer: ReturnType<typeof setInterval> | undefined;
        const sharer = () => participants.find((p) => p.id === id && p.sharingScreen);
        const size = () => {
          const [w, h, fps] = sharer()?.isLocal && share ? PRESET_SIZE[share.preset] : PRESET_SIZE['720p30'];
          return { width: w, height: h, fps, bitrateKbps: fps * 60 };
        };
        const canvas = frames ? document.createElement('canvas') : undefined;
        if (canvas) {
          canvas.width = 480;
          canvas.height = 270;
          el.srcObject = canvas.captureStream(30);
          void el.play().catch(() => {});
        }
        const draw = () => {
          const ctx = canvas?.getContext('2d');
          if (!canvas || !ctx) return;
          const bars = ['#e9f3ef', '#f3e37c', '#8fe3f0', '#7fe0be', '#d6b8ff', '#ff9fb2', '#9fc2ff'];
          const bw = canvas.width / bars.length;
          bars.forEach((c, i) => {
            ctx.fillStyle = c;
            ctx.fillRect(i * bw, 0, bw + 1, canvas.height);
          });
          ctx.fillStyle = '#0f1a1e';
          ctx.fillRect(((frameNo * 6) % (canvas.width + 80)) - 40, canvas.height * 0.62, 40, 40);
        };
        const tick = () => {
          frameNo++;
          const update: VideoUpdate = sharer() ? { status: 'playing', stats: size() } : { status: 'waiting' };
          if (update.status === 'playing') draw();
          onUpdate(update);
        };
        onUpdate({ status: 'connecting' });
        timer = setInterval(tick, 1000 / 30);
        return () => {
          clearInterval(timer);
          timer = undefined;
          if (canvas) el.srcObject = null;
        };
      },
      close() {},
    };
  }

  const subscribe =
    <K extends keyof Handlers>(key: K) =>
    async (cb: Parameters<Handlers[K]['add']>[0]): Promise<Unlisten> => {
      const set = handlers[key] as Set<typeof cb>;
      set.add(cb);
      return () => set.delete(cb);
    };

  return {
    kind: 'mock',
    get participants() {
      return participants.map((p) => ({ ...p }));
    },

    async joinRoom(params: JoinParams) {
      const errors = validateJoin(params);
      if (hasErrors(errors)) throw Object.values(errors)[0];
      if (connected || joinTimer) throw 'Você já está em uma sala.';
      if (params.password === MOCK_WRONG_PASSWORD) throw 'senha do grupo incorreta';
      const { name } = normalizeJoin(params);
      emitState({ status: 'connecting' });
      await new Promise<void>((resolve) => {
        joinTimer = setTimeout(resolve, latencyMs);
      });
      joinTimer = undefined;
      connected = true;
      deafened = false;
      share = null;
      participants = [
        { id: 'local', name, muted: false, speaking: false, sharingScreen: false, isLocal: true },
        ...FRIENDS.map((friend, i) => ({
          id: `friend-${i}`,
          name: friend,
          muted: i === 2,
          speaking: false,
          sharingScreen: remoteSharing && i === 0,
          isLocal: false,
        })),
      ];
      emitState({ status: 'connected' });
      emitParticipants();
      if (autoSpeak && speakTickMs > 0) speakTimer = setInterval(tickSpeaking, speakTickMs);
    },

    async leaveRoom() {
      stopTimers();
      connected = false;
      deafened = false;
      share = null;
      participants = [];
      emitState({ status: 'disconnected' });
      emitParticipants();
    },

    async setMuted(muted: boolean) {
      const me = requireLocal();
      if (!muted && deafened) deafened = false;
      update(me.id, { muted, speaking: muted ? false : me.speaking });
      emitParticipants();
    },

    async setDeafened(value: boolean) {
      const me = requireLocal();
      if (value && !deafened) mutedBeforeDeafen = me.muted;
      if (value) update(me.id, { muted: true, speaking: false });
      else if (deafened) update(me.id, { muted: mutedBeforeDeafen });
      deafened = value;
      emitParticipants();
    },

    async listAudioDevices() {
      return structuredClone(DEVICES);
    },

    async selectAudioDevice(kind: DeviceKind, id: string) {
      const list = kind === 'input' ? DEVICES.inputs : DEVICES.outputs;
      if (!list.some((d) => d.id === id)) throw `Dispositivo de áudio não encontrado: ${id}`;
    },

    async listScreenSources() {
      return structuredClone(SOURCES);
    },

    async startScreenShare(sourceId: string, preset: ScreenPreset) {
      if (!SOURCES.some((s) => s.id === sourceId)) throw `Fonte de tela não encontrada: ${sourceId}`;
      const me = requireLocal();
      share = { preset };
      update(me.id, { sharingScreen: true });
      emitParticipants();
    },

    async stopScreenShare() {
      const me = requireLocal();
      share = null;
      update(me.id, { sharingScreen: false });
      emitParticipants();
    },

    onRoomState: subscribe('state'),
    onParticipants: subscribe('participants'),
    openVideo: async () => videoSession(),
  };
}
