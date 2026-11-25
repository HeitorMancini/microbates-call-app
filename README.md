# Sheeshcord

App desktop (Windows) de **voz + compartilhamento de tela** para um grupo pequeno
de amigos (~7 pessoas) jogando juntos. O servidor de mídia (LiveKit) roda numa VM
gratuita da **Oracle Cloud** em `https://live.microbates.com`, protegido por uma
**senha do grupo**.

## Visão geral

- Entrar numa sala com um nome, ver quem está nela, quem está falando, mutar/ensurdecer.
- Escolher microfone/saída de áudio.
- Compartilhar um monitor ou janela (presets 1080p30, 1080p60, 720p30) e assistir à tela dos outros.

## Arquitetura

```
┌──────────────── App (Tauri 2) ────────────────┐        ┌─ VM Oracle (live.microbates.com) ─┐
│ ui/  Svelte 5 + TS  (só renderiza estado)     │ HTTPS  │ Caddy :443 (TLS automático)       │
│        ▲ eventos room://*      │ invoke(...)  │───────►│  /token,/health → token-server    │
│ src-tauri/  comandos/eventos ──┴──────────────┤  WSS   │  resto (/rtc)   → livekit :7880   │
│ crates/core   máquina de estados da sala      │───────►│                                   │
│ crates/media  áudio (cpal/opus), tela (WGC),  │ WebRTC │ livekit-server                    │
│               SDK Rust do LiveKit (libwebrtc) ├───────►│  UDP 50000–50200 · TCP 7881       │
└───────────────────────────────────────────────┘        └───────────────────────────────────┘
```

| Pasta                       | Conteúdo                                                                 |
| --------------------------- | ------------------------------------------------------------------------ |
| `ui/`                       | Interface Svelte 5 + Tailwind; `?mock=1` usa um backend falso (sem Tauri) |
| `src-tauri/`                | Shell Tauri: comandos e eventos definidos em [`CONTRACT.md`](CONTRACT.md) |
| `crates/core`               | Estado da sala, reconexão, traits `Transport`/`AudioSource`/`ScreenSource` |
| `crates/media`              | Implementações reais (áudio, captura de tela, LiveKit) + fontes sintéticas |
| `crates/token-server`       | Axum: `GET /health`, `POST /token`                                        |
| `crates/integration-tests`  | Testes contra um `livekit-server` real + token-server                    |
| `crates/load-test`          | Teste de carga/qualidade headless ([README](crates/load-test/README.md))   |
| `deploy/`                   | Docker Compose do servidor (LiveKit + token-server + Caddy) ([README](deploy/README.md)) |
| `.github/workflows/`        | `ci.yml` (testes), `deploy.yml` (imagem do token-server no GHCR), `release.yml` (instalador) |

Fluxo: UI → `join_room` → core pede token ao token-server → conecta no LiveKit →
eventos (`room://state`, `room://participants`) voltam para a UI. O vídeo da tela é recebido
direto no WebView por `livekit-client` (`viewer_credentials` → token `viewer~`), num `<video>`.

## Requisitos (desenvolvimento)

- Windows 10/11 x64
- [Rust](https://rustup.rs) estável (toolchain MSVC) + **Visual Studio Build Tools**
  com a carga "Desenvolvimento para desktop com C++" (MSVC + Windows SDK)
- [Node.js](https://nodejs.org) 22+ e [pnpm](https://pnpm.io) (`npm i -g pnpm`)
- WebView2 (já vem no Windows 11)
- [`livekit-server`](https://github.com/livekit/livekit/releases) v1.13+ (`livekit_<versão>_windows_amd64.zip`)
  em `%USERPROFILE%\.local\bin` ou no `PATH` — necessário só para os testes de integração/carga
  e para um servidor local de desenvolvimento.

Depois de instalar o Rust, se o `cargo` não estiver no PATH da sessão:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

## Como rodar

### 1. Servidor

Produção: VM Oracle Cloud Always Free com Docker Compose — guia completo (conta, VM,
firewall, DNS, `.env`, deploy automático) em [`deploy/README.md`](deploy/README.md).

Local (desenvolvimento), em dois terminais:

```powershell
livekit-server --config deploy\livekit.dev.yaml --node-ip 127.0.0.1
$env:LIVEKIT_API_KEY = "devkey"; $env:LIVEKIT_API_SECRET = "devsecretdevsecretdevsecretdevsecret"
$env:LIVEKIT_URL = "ws://127.0.0.1:7880"; cargo run -p token-server   # http://127.0.0.1:7881
```

### 2. App

```powershell
pnpm install
pnpm tauri dev                 # app completo (Vite + Tauri)
pnpm dev                       # só a UI no navegador; abra http://localhost:5173/?mock=1
```

Instalador (NSIS, instalação por usuário, sem admin):

```powershell
$env:CARGO_TARGET_DIR = "C:\t"   # caminho curto (veja a nota abaixo)
pnpm tauri build --bundles nsis
# → C:\t\release\bundle\nsis\Sheeshcord_<versão>_x64-setup.exe
```

> O build release do libwebrtc falha com `C1083 ... constexpr_parser.h` se o diretório
> de build ficar longo demais (limite `MAX_PATH` de 260 caracteres no `cl.exe`). Use um
> `CARGO_TARGET_DIR` curto, como `C:\t`. Ao criar uma tag `v*`, o workflow `release.yml`
> gera o instalador e o anexa ao GitHub Release.

## Usando

1. Instale o Sheeshcord pelo instalador da [última release](https://github.com/HeitorMancini/microbates-call-app/releases)
   (reinstalar uma versão nova atualiza a existente).
2. Servidor: `https://live.microbates.com:8443` (já vem preenchido). Informe a **senha do grupo**,
   a sala e seu nome.
3. Sem áudio/vídeo? Veja se `https://live.microbates.com:8443/health` responde `{"status":"ok"}`
   e a seção de problemas em [`deploy/README.md`](deploy/README.md).

A qualidade da tela depende do **upload de quem compartilha** (uma cópia para o servidor) e
da banda da VM (10 TB/mês grátis de saída, folgado para o grupo).

## Testes

| Nível        | Comando                                                         | Observações |
| ------------ | --------------------------------------------------------------- | ----------- |
| Lint/format  | `cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `pnpm lint` | |
| Unit Rust    | `cargo test --workspace`                                         | core, token-server, load-test… |
| Integração   | `cargo test -p integration-tests`                                | sobe um `livekit-server` local; **pula** com `SKIPPED` se ausente |
| Unit UI      | `pnpm test`                                                      | Vitest + Testing Library |
| E2E UI       | `pnpm --filter @sheeshcord/ui exec playwright install chromium` e `pnpm test:e2e` | Playwright contra o Vite em `?mock=1` |
| Carga        | `cargo test -p load-test` (fumaça, 3 clientes) · `cargo run -p load-test --release -- -n 7 --output load-report.json` | ver [crates/load-test](crates/load-test/README.md) |

O teste de carga simula 7 participantes (1 tela sintética 1080p30 + 6 espectadores),
mede bitrate, FPS, perda, jitter e tempo de conexão, e falha (código ≠ 0) se algum
limite configurado não for atingido. Perda/latência podem ser simuladas com
`clumsy` (Windows) ou `tc netem` (Linux) via `--impair-cmd/--restore-cmd`.

### CI (GitHub Actions, `windows-latest`)

- **rust**: fmt, clippy `-D warnings`, `cargo test --workspace` com `livekit-server` v1.13.11 baixado da release oficial;
- **frontend**: `pnpm install`, lint, Vitest, build;
- **e2e**: Playwright (Chromium) contra o Vite `?mock=1`; relatório HTML como artefato `playwright-report`;
- **tauri-build**: gera o instalador NSIS (artefato `sheeshcord-windows-installer`);
- **load**: só em `workflow_dispatch` ou semanalmente (segunda 06:00 UTC); artefato `load-report`.

Outros workflows: **deploy.yml** (push em `main` mexendo no token-server: imagem
x86_64 no GHCR; o servidor baixa sozinho, veja `deploy/README.md`) e **release.yml** (tag `v*` → instalador no Release).

## Limitações conhecidas

- Somente Windows (captura via Windows Graphics Capture; instalador NSIS).
- Sem TURN: quem estiver numa rede que bloqueia UDP usa ICE/TCP (porta 7881), com latência maior.
- A senha do grupo fica salva em texto puro no `localStorage` do app.
- O SDK Rust do LiveKit tem menos recursos que o JS (simulcast depende da versão do
  libwebrtc embutido; a tela codificada na GPU é publicada sem simulcast).
- A tela é codificada na GPU via Media Foundation (NVIDIA NVENC, AMD AMF, Intel QSV), na
  ordem AV1 na GPU → H.264 na GPU → software (libwebrtc: AV1/libaom → H.264/OpenH264 → VP8).
  Se o encoder da GPU falhar no meio do compartilhamento, a tela é republicada em software.
  `SHEESHCORD_HW_ENCODER=0` força software. Suporte esperado:

  | GPU | Codec na GPU | 1080p60 medido |
  |---|---|---|
  | AMD RX 7000+ (testado: RX 9060 XT) | AV1 (`AMDav1Encoder`) | 60 fps |
  | AMD RX 5000/6000 | H.264 | não medido |
  | NVIDIA RTX 40+ | AV1 | não medido |
  | NVIDIA GTX 600+ / RTX 20/30 | H.264 | não medido |
  | Intel Arc / Meteor Lake+ | AV1 | não medido |
  | sem encoder na GPU | software | load-test 1080p30: AV1 ~23 fps, VP8 ~11, H.264 ~6 (Ryzen 7 5700X) |

  A tela é decodificada pelo WebView (AV1 em software via dav1d se a GPU do espectador não
  decodificar AV1). Para conferir o encoder usado, rode com `RUST_LOG=media=info` e procure
  `screen encoding on the GPU`.
- Com muitos espectadores, a banda de saída da VM e a latência até São Paulo limitam a qualidade.
- Captura real de áudio/tela não roda no CI; lá são usadas fontes sintéticas e a UI em modo mock.
- O teste de carga roda todos os clientes numa só máquina (mede o SFU/link local, não o upload real do host).
