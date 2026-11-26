// Tipos do contrato IPC (CONTRACT.md).

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'reconnecting' | 'error';

export interface RoomState {
  status: ConnectionStatus;
  error?: string;
}

export interface Participant {
  id: string;
  name: string;
  muted: boolean;
  speaking: boolean;
  sharingScreen: boolean;
  isLocal: boolean;
}

export interface Device {
  id: string;
  name: string;
}

export interface AudioDevices {
  inputs: Device[];
  outputs: Device[];
}

export type DeviceKind = 'input' | 'output';

export interface ScreenSource {
  id: string;
  name: string;
  kind: 'monitor' | 'window';
}

export type ScreenPreset = '1080p30' | '1080p60' | '720p30';

export const SCREEN_PRESETS: readonly { id: ScreenPreset; label: string; hint: string }[] = [
  { id: '1080p30', label: '1080p · 30 fps', hint: 'Equilíbrio entre nitidez e banda' },
  { id: '1080p60', label: '1080p · 60 fps', hint: 'Jogos rápidos, exige mais da rede' },
  { id: '720p30', label: '720p · 30 fps', hint: 'Conexão fraca ou muita gente vendo' },
];

export type VideoStatus = 'connecting' | 'waiting' | 'playing' | 'stalled' | 'error';

export interface VideoStats {
  width: number;
  height: number;
  fps: number;
  bitrateKbps: number;
}

export interface VideoUpdate {
  status: VideoStatus;
  message?: string;
  stats?: VideoStats;
}

/** Conexão somente-assinatura que entrega a tela de quem compartilha direto num <video>. */
export interface VideoSession {
  /** Liga a tela de `participantId` ao elemento; devolve a função que desliga. */
  watch(participantId: string, el: HTMLVideoElement, onUpdate: (u: VideoUpdate) => void): () => void;
  close(): void;
}

export interface ViewerCredentials {
  url: string;
  token: string;
}

export interface JoinParams {
  serverUrl: string;
  room: string;
  name: string;
  /** Senha do grupo exigida pelo servidor (vazia se o servidor não pede). */
  password?: string;
}

/** Servidor e sala fixos: a tela de entrada pede só nome e senha. */
export const DEFAULT_SERVER_URL = 'wss://live.microbates.com:8443';
export const DEFAULT_ROOM = 'Call 1';

/** Salas disponíveis para troca sem sair da call. */
export const SALAS: readonly string[] = ['Call 1', 'Call 2', 'Call 3'];

export const EVENTS = {
  state: 'room://state',
  participants: 'room://participants',
  update: 'update://state',
} as const;

export type UpdateState =
  | { status: 'idle' }
  | { status: 'downloading'; percent: number | null }
  | { status: 'ready'; version: string };

export type Unlisten = () => void;

/** Superfície do backend usada pela UI: implementada pelo Tauri ou pelo mock. */
export interface Backend {
  readonly kind: 'tauri' | 'mock';
  joinRoom(params: JoinParams): Promise<void>;
  leaveRoom(): Promise<void>;
  setMuted(muted: boolean): Promise<void>;
  setDeafened(deafened: boolean): Promise<void>;
  listAudioDevices(): Promise<AudioDevices>;
  selectAudioDevice(kind: DeviceKind, id: string): Promise<void>;
  /** Ganho do microfone em porcentagem (0–200, 100 = sem alteração). */
  setMicVolume(percent: number): Promise<void>;
  listScreenSources(): Promise<ScreenSource[]>;
  startScreenShare(sourceId: string, preset: ScreenPreset, withAudio: boolean): Promise<void>;
  stopScreenShare(): Promise<void>;
  onRoomState(cb: (state: RoomState) => void): Promise<Unlisten>;
  onParticipants(cb: (participants: Participant[]) => void): Promise<Unlisten>;
  getUpdateState(): Promise<UpdateState>;
  onUpdateState(cb: (state: UpdateState) => void): Promise<Unlisten>;
  /** Instala a atualização já baixada e reinicia o app (encerra a call). */
  installUpdate(): Promise<void>;
  /** Abre o canal de vídeo (decodificado pela WebView, com aceleração de hardware). */
  openVideo(): Promise<VideoSession>;
}
