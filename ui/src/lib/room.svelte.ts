import { errorMessage } from './api';
import { normalizeJoin } from './validation';
import type {
  AudioDevices,
  Backend,
  ConnectionStatus,
  DeviceKind,
  JoinParams,
  Participant,
  ScreenPreset,
  ScreenSource,
  Unlisten,
} from './types';

/** Estado da sala em runes. Não conhece DOM: testável só com um `Backend`. */
export class RoomStore {
  status = $state<ConnectionStatus>('disconnected');
  error = $state<string | null>(null);
  participants = $state<Participant[]>([]);
  deafened = $state(false);
  roomName = $state('');
  serverUrl = $state('');
  devices = $state<AudioDevices>({ inputs: [], outputs: [] });
  selectedInput = $state('default');
  selectedOutput = $state('default');
  screenSources = $state<ScreenSource[]>([]);
  sharePreset = $state<ScreenPreset>('1080p30');
  /** Mensagem curta para a região `aria-live`. */
  announcement = $state('');
  busy = $state(false);

  local = $derived(this.participants.find((p) => p.isLocal) ?? null);
  muted = $derived(this.local?.muted ?? false);
  sharing = $derived(this.local?.sharingScreen ?? false);
  sharers = $derived(this.participants.filter((p) => p.sharingScreen));
  inRoom = $derived(this.status === 'connected' || this.status === 'reconnecting');

  readonly backend: Backend;
  #unlisten: Unlisten[] = [];
  #ready: Promise<void> | null = null;

  constructor(backend: Backend) {
    this.backend = backend;
  }

  /** Assina os eventos do backend. Idempotente. */
  start(): Promise<void> {
    this.#ready ??= (async () => {
      this.#unlisten = await Promise.all([
        this.backend.onRoomState((s) => {
          const was = this.status;
          this.status = s.status;
          if (s.status === 'error') this.error = s.error ?? 'A conexão com a sala falhou.';
          if (s.status === 'connected' && was !== 'connected') {
            this.error = null;
            this.announce(was === 'reconnecting' ? 'Conexão restabelecida.' : `Você entrou na sala ${this.roomName}.`);
          }
          if (s.status === 'reconnecting') this.announce('Conexão instável, reconectando…');
          if (s.status === 'disconnected') {
            this.deafened = false;
            this.participants = [];
          }
        }),
        this.backend.onParticipants((list) => {
          this.participants = list;
        }),
      ]);
    })();
    return this.#ready;
  }

  stop() {
    this.#unlisten.forEach((u) => u());
    this.#unlisten = [];
    this.#ready = null;
  }

  announce(message: string) {
    // Troca o texto mesmo se repetido, para leitores de tela anunciarem de novo.
    this.announcement = this.announcement === message ? `${message}\u00a0` : message;
  }

  async #run(action: () => Promise<void>): Promise<boolean> {
    try {
      await action();
      return true;
    } catch (err) {
      this.error = errorMessage(err);
      this.announce(this.error);
      return false;
    }
  }

  async join(params: JoinParams): Promise<boolean> {
    await this.start();
    const clean = normalizeJoin(params);
    this.error = null;
    this.busy = true;
    this.roomName = clean.room;
    this.serverUrl = clean.serverUrl;
    const ok = await this.#run(() => this.backend.joinRoom(clean));
    this.busy = false;
    if (!ok && this.status === 'connecting') this.status = 'error';
    return ok;
  }

  async leave() {
    await this.#run(() => this.backend.leaveRoom());
    this.deafened = false;
    this.announce('Você saiu da sala.');
  }

  async toggleMute() {
    const next = !this.muted;
    if (await this.#run(() => this.backend.setMuted(next))) {
      if (!next) this.deafened = false;
      this.announce(next ? 'Microfone desligado.' : 'Microfone ligado.');
    }
  }

  async toggleDeafen() {
    const next = !this.deafened;
    if (await this.#run(() => this.backend.setDeafened(next))) {
      this.deafened = next;
      this.announce(next ? 'Áudio da sala desligado.' : 'Áudio da sala ligado.');
    }
  }

  async loadDevices() {
    await this.#run(async () => {
      this.devices = await this.backend.listAudioDevices();
    });
  }

  async selectDevice(kind: DeviceKind, id: string) {
    if (await this.#run(() => this.backend.selectAudioDevice(kind, id))) {
      if (kind === 'input') this.selectedInput = id;
      else this.selectedOutput = id;
    }
  }

  async loadScreenSources() {
    await this.#run(async () => {
      this.screenSources = await this.backend.listScreenSources();
    });
  }

  async startShare(sourceId: string, preset: ScreenPreset): Promise<boolean> {
    this.sharePreset = preset;
    const ok = await this.#run(() => this.backend.startScreenShare(sourceId, preset));
    if (ok) this.announce('Você está compartilhando a tela.');
    return ok;
  }

  async stopShare() {
    if (await this.#run(() => this.backend.stopScreenShare())) this.announce('Compartilhamento encerrado.');
  }

  dismissError() {
    this.error = null;
    if (this.status === 'error') this.status = 'disconnected';
  }
}
