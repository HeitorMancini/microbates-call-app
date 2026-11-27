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
  UpdateState,
} from './types';

const AUDIO_KEY = 'sheeshcord:audio';

interface SavedAudio {
  noiseSuppression?: boolean;
  micSensitivity?: number;
  micVolume?: number;
}

/** Ajustes de áudio salvos; qualquer campo ausente ou inválido é ignorado. */
function readSavedAudio(): SavedAudio {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(AUDIO_KEY) ?? 'null');
    if (typeof raw !== 'object' || raw === null) return {};
    const r = raw as Record<string, unknown>;
    const number = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : undefined);
    return {
      noiseSuppression: typeof r.noiseSuppression === 'boolean' ? r.noiseSuppression : undefined,
      micSensitivity: number(r.micSensitivity),
      micVolume: number(r.micVolume),
    };
  } catch {
    return {};
  }
}

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
  /** Ganho do microfone em % (0–200). */
  micVolume = $state(100);
  /** Supressão de ruído (RNNoise) ligada. */
  noiseSuppression = $state(true);
  /** Sensibilidade do microfone 0–100; 100 = gate aberto (nada é cortado). */
  micSensitivity = $state(100);
  /** Volume local por pessoa (id → %); ausente = 100. */
  volumes = $state<Record<string, number>>({});
  screenSources = $state<ScreenSource[]>([]);
  sharePreset = $state<ScreenPreset>('1080p30');
  /** Mensagem curta para a região `aria-live`. */
  announcement = $state('');
  busy = $state(false);
  /** Troca de sala em andamento: a call continua visível enquanto dura. */
  trocando = $state(false);
  /** Última troca de sala falhou: mantém a call aberta mostrando o erro. */
  falhaTroca = $state(false);
  update = $state<UpdateState>({ status: 'idle' });

  local = $derived(this.participants.find((p) => p.isLocal) ?? null);
  muted = $derived(this.local?.muted ?? false);
  sharing = $derived(this.local?.sharingScreen ?? false);
  sharers = $derived(this.participants.filter((p) => p.sharingScreen));
  inRoom = $derived(this.status === 'connected' || this.status === 'reconnecting');
  /** O que o `App` usa para decidir entre a tela de login e a de call. */
  presente = $derived(this.inRoom || this.trocando || this.falhaTroca);

  readonly backend: Backend;
  #unlisten: Unlisten[] = [];
  #ready: Promise<void> | null = null;
  /** Últimos parâmetros normalizados usados para entrar, para repetir a troca de sala. */
  #ultimo: JoinParams | null = null;

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
        this.backend.onUpdateState((s) => {
          this.update = s;
        }),
      ]);
      this.update = await this.backend.getUpdateState().catch(() => this.update);
      await this.#applySavedAudio();
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
    if (ok) this.#ultimo = clean;
    if (!ok && this.status === 'connecting') this.status = 'error';
    return ok;
  }

  async installUpdate() {
    await this.#run(() => this.backend.installUpdate());
  }

  async leave() {
    await this.#run(() => this.backend.leaveRoom());
    this.deafened = false;
    this.falhaTroca = false;
    this.announce('Você saiu da sala.');
  }

  /** Troca de sala sem passar pela tela de login: sai e volta a entrar com os mesmos dados. */
  async trocarSala(sala: string): Promise<boolean> {
    if (this.trocando || !this.#ultimo) return false;
    if (sala === this.roomName && !this.falhaTroca) return true;
    const estavaMudo = this.muted;
    const estavaEnsurdecido = this.deafened;
    const estavaCompartilhando = this.sharing;
    this.trocando = true;
    try {
      if (!(await this.#run(() => this.backend.leaveRoom()))) {
        this.falhaTroca = true;
        return false;
      }
      const ok = await this.join({ ...this.#ultimo, room: sala });
      this.falhaTroca = !ok;
      if (ok) {
        const restaurou = await this.#run(async () => {
          if (estavaEnsurdecido) {
            await this.backend.setDeafened(true);
            this.deafened = true;
          } else if (estavaMudo) {
            await this.backend.setMuted(true);
          }
        });
        if (!restaurou) {
          this.falhaTroca = true;
          return false;
        }
        if (estavaCompartilhando) this.announce('Compartilhamento encerrado ao trocar de sala.');
      }
      return ok;
    } finally {
      this.trocando = false;
    }
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
      const devices = await this.backend.listAudioDevices();
      this.devices = devices;
      const pick = (list: { id: string }[], current: string | null | undefined, now: string) =>
        list.some((d) => d.id === now) ? now : (current ?? list[0]?.id ?? now);
      this.selectedInput = pick(devices.inputs, devices.currentInput, this.selectedInput);
      this.selectedOutput = pick(devices.outputs, devices.currentOutput, this.selectedOutput);
    });
  }

  async selectDevice(kind: DeviceKind, id: string) {
    if (await this.#run(() => this.backend.selectAudioDevice(kind, id))) {
      if (kind === 'input') this.selectedInput = id;
      else this.selectedOutput = id;
    }
  }

  async setMicVolume(percent: number) {
    const value = Math.min(200, Math.max(0, Math.round(percent)));
    if (await this.#run(() => this.backend.setMicVolume(value))) {
      this.micVolume = value;
      this.#saveAudio();
    }
  }

  async setNoiseSuppression(enabled: boolean) {
    if (await this.#run(() => this.backend.setNoiseSuppression(enabled))) {
      this.noiseSuppression = enabled;
      this.#saveAudio();
    }
  }

  async setMicSensitivity(percent: number) {
    const value = Math.min(100, Math.max(0, Math.round(percent)));
    if (await this.#run(() => this.backend.setMicSensitivity(value))) {
      this.micSensitivity = value;
      this.#saveAudio();
    }
  }

  /** Lê os ajustes de áudio salvos e os reenvia ao backend (que volta aos padrões a cada abertura). */
  async #applySavedAudio() {
    const saved = readSavedAudio();
    if (saved.noiseSuppression !== undefined) await this.setNoiseSuppression(saved.noiseSuppression);
    if (saved.micSensitivity !== undefined) await this.setMicSensitivity(saved.micSensitivity);
    if (saved.micVolume !== undefined) await this.setMicVolume(saved.micVolume);
  }

  #saveAudio() {
    try {
      localStorage.setItem(
        AUDIO_KEY,
        JSON.stringify({
          noiseSuppression: this.noiseSuppression,
          micSensitivity: this.micSensitivity,
          micVolume: this.micVolume,
        }),
      );
    } catch {
      // Armazenamento indisponível: o ajuste vale só para esta sessão.
    }
  }

  volumeOf(id: string): number {
    return this.volumes[id] ?? 100;
  }

  async setParticipantVolume(id: string, percent: number) {
    const value = Math.min(200, Math.max(0, Math.round(percent)));
    if (await this.#run(() => this.backend.setParticipantVolume(id, value))) this.volumes[id] = value;
  }

  async loadScreenSources() {
    await this.#run(async () => {
      this.screenSources = await this.backend.listScreenSources();
    });
  }

  async startShare(sourceId: string, preset: ScreenPreset, withAudio = true): Promise<boolean> {
    this.sharePreset = preset;
    const ok = await this.#run(() => this.backend.startScreenShare(sourceId, preset, withAudio));
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
