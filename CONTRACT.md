# Contrato entre módulos (Sheeshcord)

App de voz + compartilhamento de tela (sem chat) para ~7 amigos via Radmin VPN. Tauri 2 + Svelte 5 (UI) + núcleo Rust; mídia nativa (SDK Rust do LiveKit).

## token-server (HTTP, Axum, porta 7881)
- `GET /health` -> `200 {"status":"ok"}`
- `POST /token` body `{"room": string, "name": string}` -> `200 {"token": string, "url": string}`; `400` se `room`/`name` vazios ou > 64 chars; JWT LiveKit (HS256) com `video.roomJoin`, identity = name + sufixo único.
- Config por env: `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `LIVEKIT_URL` (ex.: `ws://26.1.2.3:7880`), `TOKEN_SERVER_BIND` (padrão `0.0.0.0:7881`).

## Comandos Tauri (frontend -> Rust) — `invoke`
- `join_room({ serverUrl: string, room: string, name: string })` -> `void`
- `leave_room()` -> `void`
- `set_muted({ muted: boolean })`, `set_deafened({ deafened: boolean })`
- `list_audio_devices()` -> `{ inputs: Device[], outputs: Device[] }` (`Device = { id: string, name: string }`)
- `select_audio_device({ kind: "input" | "output", id: string })`
- `list_screen_sources()` -> `ScreenSource[]` (`{ id: string, name: string, kind: "monitor" | "window" }`)
- `start_screen_share({ sourceId: string, preset: "1080p30" | "1080p60" | "720p30" })`, `stop_screen_share()`

## Eventos (Rust -> frontend) — `listen`
- `room://state` -> `{ status: "disconnected" | "connecting" | "connected" | "reconnecting" | "error", error?: string }`
- `room://participants` -> `Participant[]` com `{ id, name, muted, speaking, sharingScreen, isLocal }`
- `room://screen-frame` -> `{ participantId, width, height, jpegBase64 }` (spike; pode virar janela nativa/wgpu)

## Núcleo Rust
- `crates/core`: máquina de estados da sala, **independente de UI e de LiveKit**, com traits `Transport`, `AudioSource`, `ScreenSource` mockáveis (`async_trait`). Sem dependências de Tauri.
- `crates/media`: implementações reais (cpal, opus, windows-capture, livekit) das traits do `core` + fakes (tom senoidal, padrão de teste de vídeo) para testes/CI.
- `src-tauri`: finaliza o mapeamento comandos/eventos acima chamando `core`.
