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

export interface ScreenFrame {
  participantId: string;
  width: number;
  height: number;
  jpegBase64: string;
}

export interface JoinParams {
  serverUrl: string;
  room: string;
  name: string;
}

export const EVENTS = {
  state: 'room://state',
  participants: 'room://participants',
  screenFrame: 'room://screen-frame',
} as const;

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
  listScreenSources(): Promise<ScreenSource[]>;
  startScreenShare(sourceId: string, preset: ScreenPreset): Promise<void>;
  stopScreenShare(): Promise<void>;
  onRoomState(cb: (state: RoomState) => void): Promise<Unlisten>;
  onParticipants(cb: (participants: Participant[]) => void): Promise<Unlisten>;
  onScreenFrame(cb: (frame: ScreenFrame) => void): Promise<Unlisten>;
}
