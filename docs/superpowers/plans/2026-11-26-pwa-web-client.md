# PWA (cliente web) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Uma PWA instalável no celular com a mesma tela de login, que entra nas calls de voz (ouvir, falar, mudo/surdo) e assiste transmissões de tela.

**Architecture:** O `ui/` já é um app web; o Tauri só fornece o `Backend` (`ui/src/lib/types.ts`). Cria-se um terceiro `Backend`, `createWebBackend()`, baseado em `livekit-client` (já usado em `ui/src/lib/video.ts`) e no `POST /token` do token-server (CORS já é `permissive`). `createBackend()` escolhe: Tauri → `tauri`; navegador sem `?mock` → `web`; senão mock. Um segundo build (`vite build --mode web`) gera a PWA (manifest + service worker).

**Tech Stack:** Svelte 5, Vite, `livekit-client`, `vite-plugin-pwa`, Vitest.

**Spec:** não há spec novo; baseia-se em `docs/superpowers/specs/2026-11-25-tela-call-roda-design.md` (layout) e na decisão "PWA simples" desta conversa.

## Global Constraints

- Não alterar o app desktop: o build Tauri e seus testes continuam iguais.
- O servidor LiveKit é `wss://live.microbates.com:8443`, sala padrão `Call 1`, senha de grupo enviada em `POST {server}/token` (`{room,name,password,viewer}`).
- A PWA exige HTTPS (microfone só funciona em contexto seguro).
- Sem compartilhar tela, sem supressão de ruído nativa (RNNoise), sem OTA, sem controles de janela na PWA. Usar `noiseSuppression/echoCancellation/autoGainControl` do navegador.
- Textos da UI em português; seguir o estilo atual (Svelte 5 runes, CSS escopado).

## Review Focus

- iOS Safari suspende o áudio com tela bloqueada/aba em segundo plano: avisar o usuário, não prometer o contrário.
- `<audio>` remoto precisa de gesto do usuário para tocar (autoplay): o clique em "Entrar" deve chamar `room.startAudio()`.
- Permissão de microfone negada: entrar mesmo assim só ouvindo, com mensagem clara (não travar o login).
- Sem dispositivos de saída selecionáveis no iOS (`setSinkId` não existe): esconder o seletor de saída quando não suportado.
- Troca de rede (Wi-Fi ↔ 4G): `Reconnecting`/`Reconnected` do LiveKit devem refletir no estado, sem voltar ao login.

---

### Task 1: Seleção do backend e modo web

**Files:**
- Modify: `ui/src/lib/api.ts:71-75`
- Test: `ui/src/lib/api.test.ts` (existente, estender)

**Interfaces:**
- Produces: `shouldUseMock(search: string, tauri: boolean): boolean` (já existe) passa a coexistir com `shouldUseWeb(search: string, tauri: boolean): boolean` = `!tauri && !search.includes('mock')`; `createBackend()` devolve `createWebBackend()` quando `shouldUseWeb`.

- [ ] **Step 1: Teste falhando** — `shouldUseWeb('', false) === true`, `shouldUseWeb('?mock', false) === false`, `shouldUseWeb('', true) === false`.
- [ ] **Step 2:** `pnpm vitest run src/lib/api.test.ts` → FAIL (`shouldUseWeb` não definido).
- [ ] **Step 3:** Implementar `shouldUseWeb` e ligar em `createBackend` (import de `./web` criado na Task 2; manter o dev server atual em mock via `?mock` e `pnpm dev` continuar usando mock quando `import.meta.env.DEV` — decisão: em DEV sem Tauri continua mock, só o build `--mode web` usa web).
- [ ] **Step 4:** Rodar o teste → PASS.
- [ ] **Step 5:** Commit `feat(web): seleciona backend web fora do Tauri`.

### Task 2: `WebBackend` — entrar na call, voz, mudo/surdo, participantes

**Files:**
- Create: `ui/src/lib/web.ts` (implementa `Backend`)
- Test: `ui/src/lib/web.test.ts` (com `livekit-client` mockado via `vi.mock`)

**Interfaces:**
- Produces: `createWebBackend(fetchImpl = fetch): Backend` com `kind: 'web'` (ampliar `Backend.kind` para `'tauri' | 'mock' | 'web'`).
- Comportamento por método:
  - `joinRoom(params)`: `POST {http(s) de serverUrl}/token` com `{room,name,password}` (mesma conversão `wss→https` de `crates/media/src/token.rs::token_endpoint`); conecta `new Room({adaptiveStream:true, dynacast:true})` em `url`; `room.startAudio()`; publica o mic com `setMicrophoneEnabled(true)` (se negar, segue só ouvindo). Emite `RoomState` (`connecting` → `connected`, `reconnecting`, `error`).
  - `participants`: lista local + remotos (ignorar identidades com prefixo `viewer~`), com `speaking` vindo de `ParticipantEvent.IsSpeakingChanged`, `muted` de mic publicado/silenciado, `sharingScreen` de publicação `Track.Source.ScreenShare`. `onParticipants` emite a lista inteira a cada mudança (mesmo contrato do Tauri).
  - `setMuted`, `setDeafened` (deafen = `audioEl.muted` em todos os remotos + mute do mic), `setParticipantVolume` (`RemoteParticipant.setVolume(percent/100)`).
  - `listAudioDevices`/`selectAudioDevice`: `Room.getLocalDevices('audioinput'|'audiooutput')` + `switchActiveDevice`; `currentInput/currentOutput` do dispositivo ativo; saída vazia quando `setSinkId` indisponível.
  - Itens não suportados na web (`startScreenShare`, `listScreenSources`, `setNoiseSuppression`, `setMicSensitivity`, `getMicLevel`, `installUpdate`) rejeitam/retornam neutros (`getMicLevel` → `-100`, `getUpdateState` → `idle`, `checkForUpdate` → `false`, `getAppVersion` → versão do build).
- Consumes: `connectVideoSession` (Task 3) para `openVideo()`.

- [ ] **Step 1: Testes falhando** (`web.test.ts`): (a) `joinRoom` chama `/token` com o corpo correto e conecta com `url`/`token` devolvidos; (b) senha errada (HTTP 401) vira `RoomState {status:'error'}` com a mensagem do servidor; (c) participante `viewer~x` não aparece na lista; (d) `speaking` muda a lista; (e) `setDeafened(true)` silencia remotos e o mic; (f) permissão de mic negada ainda resulta em `connected`.
- [ ] **Step 2:** `pnpm vitest run src/lib/web.test.ts` → FAIL.
- [ ] **Step 3:** Implementar `web.ts` conforme as regras acima; reutilizar os tipos de `types.ts`. Extrair a conversão de URL para `ui/src/lib/servidor.ts` (`tokenEndpoint(url: string): string`) com teste espelhando `token_endpoint` do Rust (`wss://h` → `https://h/token`, `http://h:1` → `http://h:1/token`, `h:7881` → `http://h:7881/token`).
- [ ] **Step 4:** Testes → PASS; `pnpm lint`.
- [ ] **Step 5:** Commit `feat(web): backend LiveKit para navegador`.

### Task 3: Assistir transmissões na PWA

**Files:**
- Modify: `ui/src/lib/video.ts` (aceitar uma `Room` já conectada), `ui/src/lib/web.ts` (`openVideo`)
- Test: `ui/src/lib/video.test.ts` (existente, estender)

**Interfaces:**
- Produces: `videoSessionFromRoom(room: Room): VideoSession` — mesma lógica de `watch/close` de `connectVideoSession`, mas reusando a conexão da call (na PWA não existe o canal separado "viewer"; o áudio já é assinado pela mesma `Room`). `connectVideoSession` passa a chamar essa função, sem mudança de comportamento no desktop.

- [ ] **Step 1: Teste falhando** — `videoSessionFromRoom` com uma `Room` fake liga a trilha de screenshare ao `<video>` e emite `VideoUpdate` `connecting` → `playing`.
- [ ] **Step 2:** rodar → FAIL.
- [ ] **Step 3:** Refatorar extraindo o corpo de `connectVideoSession` (depois do `room.connect`) para `videoSessionFromRoom`; em `web.ts`, `openVideo()` devolve `videoSessionFromRoom(room)` e assina `ScreenShare` (a `Room` do `WebBackend` usa `autoSubscribe: true` para voz; a tela entra por `setSubscribed(true)`).
- [ ] **Step 4:** `pnpm vitest run` completo → PASS (testes do desktop intactos).
- [ ] **Step 5:** Commit `feat(web): assistir tela compartilhada`.

### Task 4: Ajustes de UI para toque e telas pequenas

**Files:**
- Modify: `ui/src/components/RoomScreen.svelte`, `Palco.svelte`, `JoinScreen.svelte`, `DeviceSettings.svelte`, `ui/src/lib/janela.ts`
- Test: `ui/src/components/RoomScreen.test.ts`, `Palco.test.ts`

**Interfaces:**
- Consumes: `Backend.kind === 'web'`.
- Regras: esconder `ControlesJanela`/arrasto/Tauri-only quando `kind === 'web'`; esconder compartilhar tela, supressão de ruído, sensibilidade, medidor, "Verificar atualizações" e seletor de saída (se `setSinkId` ausente) em Configurações; `ajuste de volume` por pessoa abre por **toque longo** (500 ms) no blob, além do botão direito; `Palco` usa `largura/altura` de `visualViewport`; fundo ASCII desligado por padrão no mobile (`matchMedia('(pointer: coarse)')`) para poupar bateria.

- [ ] **Step 1: Testes falhando** — com backend `kind: 'web'`: não renderiza o botão de compartilhar tela nem "Verificar atualizações"; toque longo no blob abre o ajuste de volume (`pointerdown` + timers falsos).
- [ ] **Step 2:** rodar → FAIL.
- [ ] **Step 3:** Implementar as regras; layout em retrato: rodapé com botões 56 px, `safe-area-inset-*` (`env(safe-area-inset-bottom)`).
- [ ] **Step 4:** testes + `pnpm lint` → PASS.
- [ ] **Step 5:** Commit `feat(web): UI adaptada para toque`.

### Task 5: PWA (manifest, service worker, build web)

**Files:**
- Modify: `ui/package.json` (script `build:web`), `ui/vite.config.ts`
- Create: `ui/public/icons/` (192, 512, maskable), `ui/public/manifest.webmanifest` (ou via plugin)
- Add dependency: `vite-plugin-pwa`

**Interfaces:**
- Produces: `pnpm build:web` → `ui/dist-web/` com `manifest` (`name: "μChat"`, `display: "standalone"`, `theme_color` = `--color-casco`), service worker que só faz cache do shell estático (nunca de `/token` nem de WebSocket).

- [ ] **Step 1:** `pnpm add -D vite-plugin-pwa` em `ui/`.
- [ ] **Step 2:** Configurar o plugin apenas em `--mode web` (o build Tauri não muda); `registerType: 'autoUpdate'` (a PWA se atualiza sozinha, substitui o OTA).
- [ ] **Step 3:** `pnpm build:web` e `pnpm build` (Tauri) → ambos passam; confirmar que `dist/` do Tauri não contém service worker.
- [ ] **Step 4:** Servir `dist-web` localmente (`pnpm preview --mode web`) e checar no navegador: manifest válido, instalável.
- [ ] **Step 5:** Commit `feat(web): manifest e service worker`.

### Task 6: Hospedagem e publicação

**Files:**
- Create: `.github/workflows/pwa.yml`
- Modify: `README.md` (seção "PWA")

**Interfaces:**
- Produces: deploy de `ui/dist-web` em GitHub Pages com domínio próprio **chat.microbates.com** (HTTPS grátis). DNS: registro `CNAME chat` → `heitormancini.github.io` (sem IP/porta); arquivo `ui/public/CNAME` com `chat.microbates.com`; `base` do Vite = `/`. Dispara a cada push na `main` que mexa em `ui/**`.

- [ ] **Step 1:** Workflow: checkout, `pnpm install --frozen-lockfile`, `pnpm build:web` (`base` configurado para o subcaminho do Pages), `actions/upload-pages-artifact` + `actions/deploy-pages`.
- [ ] **Step 2:** Habilitar Pages (Settings → Pages → Source: GitHub Actions) — passo manual do usuário.
- [ ] **Step 3:** Abrir a URL no celular, entrar numa call com o desktop e verificar: ouve e fala, indicador de fala, mudo/surdo, assistir tela, instalar na tela inicial.
- [ ] **Step 4:** Documentar no README (limitações do iOS: áudio em segundo plano, sem compartilhar tela).
- [ ] **Step 5:** Commit `ci(web): publica a PWA`.

---

## Self-Review

- **Cobertura:** mesma tela de login (Tasks 1 e 4, reaproveita `JoinScreen`), voz (Task 2), assistir transmissão (Task 3), instalável (Task 5), hospedagem HTTPS (Task 6).
- **Consistência de tipos:** `createWebBackend`, `videoSessionFromRoom`, `tokenEndpoint` e `Backend.kind` aparecem com a mesma assinatura em todas as tasks.
- **Proporção:** 6 tasks; a Task 2 é a maior e a mais arriscada (contrato `participants` + estados).
- **Riscos abertos:** no iOS o áudio para com a tela bloqueada, e não há compartilhamento de tela no navegador móvel (fora do escopo); o ajuste de volume por pessoa usa `setVolume` do LiveKit, que no Safari só funciona com WebAudio ativo (verificar na Task 2).
