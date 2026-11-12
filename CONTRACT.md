# Contrato entre módulos (Sheeshcord)

App de voz + compartilhamento de tela (sem chat) para ~7 amigos, servidor LiveKit em `https://live.microbates.com` (Oracle Cloud). Tauri 2 + Svelte 5 (UI) + núcleo Rust; mídia nativa (SDK Rust do LiveKit).

## token-server (HTTP, Axum, porta 7881; em produção 127.0.0.1:7890 atrás do Caddy)
- `GET /health` -> `200 {"status":"ok"}`
- `POST /token` body `{"room": string, "name": string, "password"?: string, "viewer"?: bool}` -> `200 {"token": string, "url": string}`; `400` se `room`/`name` vazios ou > 64 chars; `401` se `ACCESS_PASSWORD` estiver definida e a senha não bater; `429` após 10 senhas erradas do mesmo IP em 15 min. JWT LiveKit (HS256) com `video.roomJoin`, identity = name + sufixo único.
- Config por env: `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `LIVEKIT_URL` (ex.: `wss://live.microbates.com`), `ACCESS_PASSWORD` (senha do grupo; vazia = sem senha, só para dev), `TOKEN_SERVER_BIND` (padrão `0.0.0.0:7881`).

## Comandos Tauri (frontend -> Rust) — `invoke`
- `join_room({ serverUrl: string, room: string, name: string, password?: string })` -> `void`
- `leave_room()` -> `void`
- `set_muted({ muted: boolean })`, `set_deafened({ deafened: boolean })`
- `list_audio_devices()` -> `{ inputs: Device[], outputs: Device[] }` (`Device = { id: string, name: string }`)
- `select_audio_device({ kind: "input" | "output", id: string })`
- `list_screen_sources()` -> `ScreenSource[]` (`{ id: string, name: string, kind: "monitor" | "window" }`)
- `start_screen_share({ sourceId: string, preset: "1080p30" | "1080p60" | "720p30" })`, `stop_screen_share()`

## Eventos (Rust -> frontend) — `listen`
- `room://state` -> `{ status: "disconnected" | "connecting" | "connected" | "reconnecting" | "error", error?: string }`
- `room://participants` -> `Participant[]` com `{ id, name, muted, speaking, sharingScreen, isLocal }`
- Vídeo: comando `viewer_credentials` -> `{ url, token }` (token `viewer~`, só assina); a UI conecta com `livekit-client` e exibe num `<video>`. Não há evento de frames.

## Núcleo Rust
- `crates/core`: máquina de estados da sala, **independente de UI e de LiveKit**, com traits `Transport`, `AudioSource`, `ScreenSource` mockáveis (`async_trait`). Sem dependências de Tauri.
- `crates/media`: implementações reais (cpal, opus, windows-capture, livekit) das traits do `core` + fakes (tom senoidal, padrão de teste de vídeo) para testes/CI.
- `src-tauri`: finaliza o mapeamento comandos/eventos acima chamando `core`.
