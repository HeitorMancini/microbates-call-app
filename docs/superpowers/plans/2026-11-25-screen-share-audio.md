# Áudio no compartilhamento de tela — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publicar o áudio da origem compartilhada (app da janela, ou sistema inteiro no monitor, sempre excluindo o microchat) junto com o vídeo da tela.

**Architecture:** `WgcScreen::start` abre uma captura de loopback por processo do WASAPI (include = PID da janela; exclude = PID do próprio app para monitor) e empurra PCM 48 kHz mono no `MediaHub`. O transporte publica uma 2ª faixa `TrackSource::ScreenshareAudio` alimentada por esse sink. O receptor reaproveita o mixer, com chave `"{identidade}#screen"`.

**Tech Stack:** Rust (crate `windows` 0.62, já usada), LiveKit `NativeAudioSource`, Tauri, Svelte.

**Spec:** `docs/superpowers/specs/2026-11-25-screen-share-audio-design.md`

## Global Constraints

- Windows 10 2004+ para loopback por processo; abaixo disso o share continua só com vídeo e a UI avisa.
- Áudio da tela: PCM i16 mono 48 kHz (`SAMPLE_RATE` do hub), sem AEC/NS/AGC.
- Áudio ligado por padrão; falha de áudio nunca derruba o vídeo.
- O áudio do próprio processo do microchat nunca é capturado.
- Streams do WASAPI vivem em thread própria (não `Send`).
- Textos de UI em português; usar `announce` para avisos.
- Desvio do spec (decisão de planejamento): o PID da janela é resolvido dentro de `screen.rs` a partir do HWND do `source_id`, então `ScreenSourceInfo` **não** muda.

## Review Focus

- Monitor sem nenhum áudio tocando: faixa publicada, silêncio, sem erro.
- Janela cujo app fecha durante o share: captura de áudio para, sem pânico.
- Parar/reiniciar share várias vezes: sem faixa órfã nem thread vazada.
- Participante sai enquanto compartilha: chave `#screen` removida do mixer.
- Windows antigo (loopback por processo indisponível): vídeo publica normalmente.

---

### Task 1: Contratos `with_audio` (core, mocks, fakes, Tauri)

**Files:**
- Modify: `crates/core/src/traits.rs:53,72`, `crates/core/src/room.rs:140,261,487`, `crates/core/src/mock.rs:166,258-290`, `crates/media/src/fake.rs:261`, `src-tauri/src/room.rs:241-260`, `src-tauri/src/lib.rs:150`
- Test: `crates/core/tests/room.rs`

**Interfaces:**
- Produces: `Transport::publish_screen(&self, preset: ScreenPreset, with_audio: bool)`; `ScreenSource::start(&self, source_id: &str, preset: ScreenPreset, with_audio: bool)`; `RoomHandle::start_screen_share(&self, source: &str, preset: ScreenPreset, with_audio: bool)`; comando Tauri `start_screen_share(source_id, preset, with_audio)`. Mocks guardam `with_audio` (`active()` passa a retornar `(String, ScreenPreset, bool)`).

- [ ] **Step 1: Test** em `crates/core/tests/room.rs`: `screen_share_forwards_audio_flag` — `start_screen_share("monitor-0", P720Fps30, false)` e `assert_eq!(screen.active(), Some(("monitor-0".into(), ScreenPreset::P720Fps30, false)))`; e o transporte mock registra o mesmo flag.
- [ ] **Step 2:** `cargo test -p <core> screen_share_forwards_audio_flag` → FAIL (não compila).
- [ ] **Step 3:** Adicionar o parâmetro em todas as assinaturas acima e atualizar chamadas existentes (`true` onde o teste não se importa).
- [ ] **Step 4:** `cargo test --workspace --exclude load-test` → PASS (ajustar `smoke.rs`/`load-test` se compilar com a assinatura antiga).
- [ ] **Step 5:** Commit `feat(core): thread with_audio through screen share`.

### Task 2: Hub — sink de áudio da tela e mixer

**Files:**
- Modify: `crates/media/src/hub.rs` (próximo a `set_mic_sink`/`remove_remote`)
- Test: `crates/media/src/hub.rs` (módulo `tests`)

**Interfaces:**
- Produces: `MediaHub::set_screen_audio_sink(&self, sink: Option<mpsc::Sender<Vec<i16>>>)`; `MediaHub::push_screen_audio(&self, samples: &[i16]) -> bool` (false sem sink ou canal cheio, nunca bloqueia); `pub fn screen_audio_key(identity: &str) -> String` retornando `"{identity}#screen"`; `remove_remote(id)` também remove `screen_audio_key(id)`.

- [ ] **Step 1: Tests** `screen_audio_is_dropped_without_sink` (`!push_screen_audio(&[1;480])`), `screen_audio_reaches_sink` (recebe `vec![1;480]`), `remove_remote_also_drops_screen_audio` (push com chave `screen_audio_key("a")`, `remove_remote("a")`, mixer vazio).
- [ ] **Step 2:** `cargo test -p media hub::tests::screen_audio` → FAIL.
- [ ] **Step 3:** Implementar espelhando o `mic_sink` (Mutex de `Option<Sender>`, `try_send`).
- [ ] **Step 4:** Rodar de novo → PASS. Commit.

### Task 3: Transporte — publicar/receber a faixa `ScreenshareAudio`

**Files:**
- Modify: `crates/media/src/transport.rs` (`ScreenTrack` ~l.181, `publish_screen` l.472, `unpublish_screen`, `RoomEvent::TrackSubscribed` l.870, `read_audio` l.916)
- Test: `crates/media/tests/livekit.rs` (ignorado sem servidor, seguir padrão) e unit test para a escolha de chave

**Interfaces:**
- Consumes: `set_screen_audio_sink`, `screen_audio_key` (Task 2), `publish_screen(preset, with_audio)` (Task 1).
- Produces: `ScreenTrack.audio: Option<AudioPublish>` (publicação + task do pump); função pura `audio_mixer_key(identity: &str, source: TrackSource) -> String` (`ScreenshareAudio` → `screen_audio_key`, senão identidade).

- [ ] **Step 1: Test** `audio_mixer_key_separates_screen_audio`: `Microphone`→`"bob"`, `ScreenshareAudio`→`"bob#screen"`.
- [ ] **Step 2:** Rodar → FAIL.
- [ ] **Step 3:** Implementar `audio_mixer_key` e usá-la no `TrackSubscribed` (`publication.source()`); em `publish_screen`, se `with_audio`, criar `NativeAudioSource` (mesmos parâmetros do mic, sem processamento), publicar com `TrackSource::ScreenshareAudio`, spawn do pump do canal → `capture_frame`, `hub.set_screen_audio_sink(Some(tx))`. Erro de publicação: `tracing::warn!` e seguir sem áudio. Em `unpublish_screen` e no início de `publish_screen`, abortar pump, `set_screen_audio_sink(None)` e despublicar.
- [ ] **Step 4:** `cargo test -p media audio_mixer_key` → PASS; `cargo build -p media`. Commit.

### Task 4: Captura de loopback por processo (`screen_audio.rs`)

**Files:**
- Create: `crates/media/src/screen_audio.rs` (`#[cfg(windows)]`)
- Modify: `crates/media/src/lib.rs` (mod), `crates/media/Cargo.toml:34` (features de `windows`: `Win32_Media_Audio`, `Win32_System_Com`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Threading` conforme o compilador exigir)
- Test: no próprio arquivo

**Interfaces:**
- Produces:
  - `pub enum AudioTarget { IncludeProcess(u32), ExcludeProcess(u32) }`
  - `pub struct ScreenAudioCapture` com `pub fn start(target: AudioTarget, hub: MediaHub) -> Result<Self, BoxError>`; `Drop` encerra a thread.
  - `fn pid_for_hwnd(hwnd: isize) -> Option<u32>` (`GetWindowThreadProcessId`)
  - `fn to_mono_i16_48k(input: &[f32], channels: usize, rate: u32, out: &mut Vec<i16>)` (puro, com resample linear simples)

- [ ] **Step 1: Tests** puros para `to_mono_i16_48k`: estéreo 48 kHz `[0.5, -0.5, …]` → média 0; mono 44,1 kHz produz ≈ `n*48000/44100` amostras; clamp de `±1.5` em `i16::MAX/MIN`.
- [ ] **Step 2:** Rodar → FAIL.
- [ ] **Step 3:** Implementar a conversão. Depois a captura: thread dedicada com `CoInitializeEx`, `ActivateAudioInterfaceAsync(VAD\Process_Loopback, …)` com `AUDIOCLIENT_ACTIVATION_PARAMS` (modo include/exclude + `TargetProcessId`, árvore de processos), `AUDCLNT_STREAMFLAGS_LOOPBACK | EVENTCALLBACK`, formato fixo float 48 kHz estéreo, loop de eventos chamando `hub.push_screen_audio`, parada por flag atômica.
- [ ] **Step 4:** `cargo test -p media screen_audio` → PASS. Teste manual: tocar áudio em um app e ver amostras não nulas (log de nível de pico em `debug!`). Commit.

### Task 5: Integrar em `WgcScreen::start`

**Files:**
- Modify: `crates/media/src/screen.rs` (struct `WgcScreen` l.105, `start` l.170, `stop_capture`)
- Test: `crates/media/src/screen.rs` (módulo `tests`)

**Interfaces:**
- Consumes: `ScreenAudioCapture`, `AudioTarget`, `pid_for_hwnd` (Task 4); `ScreenSource::start(.., with_audio)` (Task 1).
- Produces: função pura `audio_target(source_id: &str, own_pid: u32, window_pid: impl Fn(isize) -> Option<u32>) -> Option<AudioTarget>`: `monitor:*` → `ExcludeProcess(own_pid)`; `window:<hwnd>` → `IncludeProcess(pid)`; PID desconhecido ou id inválido → `None`.

- [ ] **Step 1: Tests** `audio_target_monitor_excludes_own_pid`, `audio_target_window_includes_owner`, `audio_target_unknown_window_is_none`.
- [ ] **Step 2:** Rodar → FAIL.
- [ ] **Step 3:** Implementar `audio_target`; em `start`, se `with_audio` e target presente, iniciar `ScreenAudioCapture` guardada em novo campo `audio: Mutex<Option<ScreenAudioCapture>>` (erro → `tracing::warn!`, segue sem áudio); `stop_capture` também derruba o áudio. Quando o app da janela fechar, o áudio para junto com a captura de vídeo (derrubar no mesmo ponto em que o handler de fechamento do WGC encerra).
- [ ] **Step 4:** `cargo test -p media screen::tests` → PASS. Commit.

### Task 6: UI — opção "Compartilhar áudio"

**Files:**
- Modify: `ui/src/lib/types.ts:104`, `ui/src/lib/api.ts:31`, `ui/src/lib/mock.ts:251`, `ui/src/lib/room.svelte.ts:194`, `ui/src/components/ShareSettings.svelte:22`
- Test: `ui/src/lib/api.test.ts`, `ui/src/lib/mock.test.ts`

**Interfaces:**
- Produces: `startScreenShare(sourceId: string, preset: ScreenPreset, withAudio: boolean): Promise<void>` (invoke `{ sourceId, preset, withAudio }`); `startShare(sourceId, preset, withAudio = true)`.

- [ ] **Step 1: Test** em `api.test.ts`: espera `['start_screen_share', { sourceId: 'monitor:0', preset: '1080p60', withAudio: true }]`; em `mock.test.ts`, chamada com `false` ainda funciona.
- [ ] **Step 2:** `pnpm vitest run ui/src/lib` → FAIL.
- [ ] **Step 3:** Atualizar assinaturas e adicionar checkbox "Compartilhar áudio" (marcado por padrão) em `ShareSettings.svelte`, com label acessível.
- [ ] **Step 4:** Rodar vitest e `pnpm check` → PASS. Commit.

### Task 7: Verificação ponta a ponta e docs

**Files:** Modify `README.md` (seção de compartilhamento), `PRIVACY.md` se mencionar captura.

- [ ] **Step 1:** `cargo test --workspace` e `pnpm vitest run` → tudo PASS.
- [ ] **Step 2:** Manual com duas contas: (a) janela de vídeo → só o áudio dele chega; (b) monitor com música + voz dos participantes → voz não retorna (sem eco), (c) fechar o app da janela e parar/reiniciar o share 5×.
- [ ] **Step 3:** Documentar requisito Windows 10 2004+ e commit `docs: screen share audio`.
