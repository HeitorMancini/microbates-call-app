# Sheeshcord

App desktop (Windows) de **voz + compartilhamento de tela** para um grupo pequeno
de amigos (~7 pessoas) jogando juntos pela **Radmin VPN**. Sem chat, sem nuvem:
um dos amigos (o *host*) roda o servidor de mídia no próprio PC.

## Visão geral

- Entrar numa sala com um nome, ver quem está nela, quem está falando, mutar/ensurdecer.
- Escolher microfone/saída de áudio.
- Compartilhar um monitor ou janela (presets 1080p30, 1080p60, 720p30) e assistir à tela dos outros.

## Arquitetura

```
┌──────────────── App (Tauri 2) ────────────────┐        ┌──── PC do host ────┐
│ ui/  Svelte 5 + TS  (só renderiza estado)     │        │ token-server :7881 │
│        ▲ eventos room://*      │ invoke(...)  │  HTTP  │  POST /token → JWT │
│ src-tauri/  comandos/eventos ──┴──────────────┼───────►│                    │
│ crates/core   máquina de estados da sala      │        │ livekit-server     │
│ crates/media  áudio (cpal/opus), tela (WGC),  │ WebRTC │  :7880 sinalização │
│               SDK Rust do LiveKit (libwebrtc) ├───────►│  UDP 50000–50100   │
└───────────────────────────────────────────────┘        └────────────────────┘
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
| `infra/`                    | `livekit.yaml` e scripts PowerShell para o host ([README](infra/README.md)) |
| `.github/workflows/ci.yml`  | CI no `windows-latest`                                                   |

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
  em `%USERPROFILE%\.local\bin` ou no `PATH` — necessário só para o host e para
  os testes de integração/carga.

Depois de instalar o Rust, se o `cargo` não estiver no PATH da sessão:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

## Como rodar

### 1. Servidor (só no PC do host)

```powershell
.\infra\start-server.ps1 -OpenFirewall   # 1ª vez, PowerShell como administrador
.\infra\start-server.ps1                 # detecta o IP 26.x.x.x do Radmin
.\infra\stop-server.ps1
```

Detalhes (portas, chaves, logs) em [`infra/README.md`](infra/README.md).

### 2. App

```powershell
pnpm install
pnpm tauri dev                 # app completo (Vite + Tauri)
pnpm dev                       # só a UI no navegador; abra http://localhost:5173/?mock=1
```

Instalador (NSIS, instalação por usuário, sem admin):

```powershell
$env:CARGO_TARGET_DIR = "C:\skt"   # caminho curto (veja a nota abaixo)
pnpm tauri build --bundles nsis
# → C:\skt\release\bundle\nsis\Sheeshcord_0.1.0_x64-setup.exe
```

> O build release do libwebrtc falha com `C1083 ... constexpr_parser.h` se o diretório
> de build ficar longo demais (limite `MAX_PATH` de 260 caracteres no `cl.exe`). Use um
> `CARGO_TARGET_DIR` curto, como `C:\skt`.

## Usando com o Radmin VPN

1. Todos instalam o [Radmin VPN](https://www.radmin-vpn.com) e entram na mesma rede.
2. O host anota o próprio IP do Radmin (`26.x.x.x`) e roda `.\infra\start-server.ps1`
   (ou `-NodeIp 26.x.x.x` se tiver mais de um adaptador).
3. Na primeira vez o host libera o firewall (`-OpenFirewall`): TCP 7880–7882 e UDP 50000–50100.
4. Cada amigo abre o Sheeshcord e usa como servidor `http://26.x.x.x:7881`
   (IP do host), escolhe um nome e entra na sala.
5. Sem áudio/vídeo? Confira se o host está online no Radmin, se as portas UDP estão
   liberadas e se `http://26.x.x.x:7881/health` responde `{"status":"ok"}`.

O LiveKit anuncia apenas o IP do Radmin (`use_external_ip: false`), então o tráfego
fica dentro da VPN. A qualidade da tela é limitada pelo **upload do host**: com 6
espectadores, o SFU envia uma cópia para cada um.

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

## Limitações conhecidas

- Somente Windows (captura via Windows Graphics Capture; instalador NSIS).
- Feito para LAN/VPN: sem TURN, sem TLS e com chaves de desenvolvimento
  (`devkey`/`devsecret…`) — **não exponha o servidor na internet** sem trocar as chaves e configurar TLS.
- O SDK Rust do LiveKit tem menos recursos que o JS (encode por hardware/simulcast
  dependem da versão do libwebrtc embutido).
- A tela é enviada em H.264 (encoder por hardware quando disponível, fallback VP8) e
  decodificada pelo WebView; o desempenho real em 1080p60 depende da GPU do host.
- Upload do host e latência do Radmin limitam a qualidade com muitos espectadores.
- Captura real de áudio/tela não roda no CI; lá são usadas fontes sintéticas e a UI em modo mock.
- O teste de carga roda todos os clientes numa só máquina (mede o SFU/link local, não o upload real do host).
