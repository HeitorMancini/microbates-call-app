import type { JoinParams } from './types';

export const MAX_FIELD_LEN = 64;

export type JoinErrors = Partial<Record<keyof JoinParams, string>>;

const URL_RE = /^(wss?|https?):\/\/[^\s/]\S*$/i;

/** Mesmas regras do backend (src-tauri/src/room.rs) e do token-server. */
export function validateJoin(params: JoinParams): JoinErrors {
  const errors: JoinErrors = {};
  const serverUrl = params.serverUrl.trim();
  if (!serverUrl) errors.serverUrl = 'Informe o endereço do servidor.';
  else if (!URL_RE.test(serverUrl))
    errors.serverUrl = 'Use um endereço como ws://26.1.2.3:7880 (ws, wss, http ou https).';

  for (const [key, label] of [
    ['room', 'o nome da sala'],
    ['name', 'seu nome'],
  ] as const) {
    const value = params[key].trim();
    if (!value) errors[key] = `Informe ${label}.`;
    else if ([...value].length > MAX_FIELD_LEN) errors[key] = `Use no máximo ${MAX_FIELD_LEN} caracteres.`;
  }
  return errors;
}

export function hasErrors(errors: JoinErrors): boolean {
  return Object.keys(errors).length > 0;
}

export function normalizeJoin(params: JoinParams): JoinParams {
  return { serverUrl: params.serverUrl.trim(), room: params.room.trim(), name: params.name.trim() };
}
