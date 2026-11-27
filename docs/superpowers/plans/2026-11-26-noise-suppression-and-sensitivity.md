# Supressão de ruído e sensibilidade do microfone — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trocar a supressão de ruído básica do WebRTC por RNNoise e adicionar um noise gate controlado por "sensibilidade", com medidor de nível ao vivo nas configurações.

**Architecture:** `CapturePipeline` (em `audio.rs`) passa a rodar `APM (eco, AGC, HPF; sem NS) → Denoiser (RNNoise) → NoiseGate → hub.push_mic` (o ganho do `micVolume` já é aplicado dentro de `push_mic`, depois do gate). Os dois ajustes ficam em atomics no `MediaHub`, igual ao `mic_volume`, e o pipeline os lê a cada chunk de 10 ms. O medidor lê um atômico de nível que o pipeline atualiza.

**Tech Stack:** Rust (`nnnoiseless`, `cpal`, LiveKit APM), Tauri, Svelte 5.

**Spec:** `docs/superpowers/specs/2026-11-25-ota-and-mic-processing-design.md` (seção 2). Volume do microfone e volume por pessoa já estão implementados (`271e531`, `0efb16d`) e fora deste plano.

## Estado atual relevante

- `AudioProcessing::default()` liga `noise_suppression` do WebRTC; `open_input` não conhece o hub além do `push_mic`.
- O ganho do microfone fica em `MediaHub::push_mic` (`hub.rs`), **não** em `audio.rs` como o spec descreve. Ele roda depois do ponto onde o gate vai ficar.
- O indicador de fala local (`SpeechDetector`, limiar fixo -40 dBFS) lê os chunks que saem de `push_mic`. Chunk cortado pelo gate chega zerado, então os dois concordam sem mexer no detector.
- `DeviceSettings` só abre dentro da call, e o engine de áudio só roda em call. O medidor só precisa funcionar nesse caso.
- Nada de `micVolume` é persistido hoje (volta a 100% a cada abertura).

## Global Constraints

- Quadros de 10 ms, 48 kHz mono i16 (`SAMPLES_PER_10MS = 480`), idêntico ao `nnnoiseless::DenoiseState::FRAME_SIZE`.
- `nnnoiseless` espera `f32` na **escala de i16** (±32768), não normalizado em ±1.
- Nada de alocação nem de `Mutex` no caminho de áudio além do que já existe; ajustes por atomics.
- Padrão para quem nunca mexeu: supressão ligada, sensibilidade 100 (gate aberto, nada é cortado).
- Textos de UI em português; usar `announce` para avisos.
- Desligar a supressão não pode reaproveitar estado antigo do RNNoise: ao religar, reiniciar o `DenoiseState`.

## Decisões de planejamento (confirmar antes de executar)

1. **Gate antes do ganho**, não depois (o spec diz o contrário). Assim o limiar independe do slider de volume e o medidor lê o sinal pré-ganho. Custa zero código extra porque o ganho já está em `push_mic`.
2. **Mapeamento da sensibilidade**: 100 = gate desligado; abaixo disso, limiar em dBFS = `-60 + (100 - s) * 0.4` (99 → -59,6 dBFS; 0 → -20 dBFS). Função única em Rust, com a mesma tabela de valores testada no TS para a marca do medidor.
3. **Lookahead de 1 chunk (10 ms)** no gate para a decisão de abrir olhar o chunk seguinte. É o que realmente evita cortar o começo das palavras (objetivo do spec); o release de 250 ms só protege o final. Custo: 10 ms de latência. *Implementado sempre ligado* (também com sensibilidade 100, só atrasando o chunk sem alterá-lo), para não haver transição de estado entre "com" e "sem" atraso.
4. **Persistência** em `localStorage` (`sheeshcord:audio`) com `noiseSuppression`, `micSensitivity` e `micVolume`; reenvio ao backend em `RoomStore.start()`. Incluir `micVolume` é um pequeno acréscimo ao escopo, porque o mecanismo é o mesmo.
5. **Medidor por polling**: comando `mic_level` consultado a ~15 Hz só enquanto o painel está aberto, em vez de um evento permanente.

## Review Focus

- Ruído branco/teclado em silêncio com sensibilidade < 100: sai silêncio exato, sem "chiado" residual.
- Primeira sílaba depois de silêncio longo: não é cortada com sensibilidade moderada.
- Sensibilidade 100 é identidade bit a bit (com supressão desligada) — não pode haver regressão para quem não mexe.
- Trocar de microfone com a call aberta (`select_device` recria o pipeline): denoiser e gate reiniciam sem estalo.
- Mudo (`capture_muted`) continua sem enviar nada e o medidor não congela num valor alto.
- Uso de CPU do RNNoise em build `dev` (ver Task 1 Step 4).

---

### Task 1: `Denoiser` (RNNoise)

**Files:**
- Modify: `crates/media/Cargo.toml`, `Cargo.toml` (profile), `crates/media/src/lib.rs` (`mod denoise;`)
- Create: `crates/media/src/denoise.rs`

**Interfaces:**
- Produces: `Denoiser::new()`, `Denoiser::process(&mut self, chunk: &mut [i16])` para chunks de exatamente `SAMPLES_PER_10MS`; `Denoiser::reset(&mut self)`.

- [ ] **Step 1: Tests** (módulo `tests`): `silence_stays_silence` (480 zeros × 50 → pico ≤ 1); `white_noise_loses_energy` (ruído branco determinístico em ±3000 × 100 chunks, RMS da saída dos últimos 50 < 50% do da entrada); `preserves_chunk_length`; `reset_restarts_state` (processar ruído, `reset()`, e o primeiro chunk de saída volta ao comportamento do primeiro quadro).
- [ ] **Step 2:** `cargo test -p media denoise` → FAIL (módulo inexistente).
- [ ] **Step 3:** Adicionar `nnnoiseless = { version = "0.5", default-features = false }` e implementar: converter i16→f32 (escala i16), `DenoiseState::process_frame`, f32→i16 com saturação. O primeiro quadro de saída do RNNoise tem fade-in; documentar e aceitar.
- [ ] **Step 4:** Em `Cargo.toml` raiz, `[profile.dev.package.nnnoiseless] opt-level = 3` (RNNoise em debug não roda em tempo real).
- [ ] **Step 5:** `cargo test -p media denoise` → PASS. Commit `feat(media): RNNoise denoiser`.

### Task 2: `NoiseGate`

**Files:**
- Modify: `crates/media/src/denoise.rs` (ou novo `gate.rs` se passar de ~150 linhas), `lib.rs`

**Interfaces:**
- Produces: `NoiseGate::new()`; `NoiseGate::process(&mut self, chunk: &mut Vec<i16>, sensitivity: u32)` (devolve o chunk com atraso de 1 se o lookahead estiver ligado); `pub fn threshold_dbfs(sensitivity: u32) -> Option<f32>` (`None` para 100); `pub fn rms_dbfs(chunk: &[i16]) -> f32`.

- [ ] **Step 1: Tests**: `sensitivity_100_never_cuts` (qualquer chunk passa idêntico, até ruído baixo); `opens_on_first_chunk_above_threshold`; `closed_gate_outputs_exact_silence`; `release_holds_then_closes` (≈250 ms após o último chunk alto, com suavização, sem degrau); `threshold_table` (100 → None, 99 → -59,6, 50 → -40, 0 → -20); `lookahead_keeps_onset` (chunk baixo imediatamente antes de um alto sai audível).
- [ ] **Step 2:** `cargo test -p media gate` → FAIL.
- [ ] **Step 3:** Implementar ataque instantâneo, release ~250 ms, ganho suavizado por amostra (rampa de ~5 ms) para não picotar.
- [ ] **Step 4:** Lookahead de 1 chunk (decisão 3). Re-rodar testes → PASS. Commit `feat(media): noise gate with sensitivity`.

### Task 3: Ligar no `CapturePipeline` e no hub

**Files:**
- Modify: `crates/media/src/audio.rs`, `crates/media/src/hub.rs`, `crates/media/src/lib.rs`
- Test: `audio.rs` (módulo `tests`), `hub.rs` (módulo `tests`)

**Interfaces:**
- Produces: `MediaHub::set_noise_suppression(bool)`, `noise_suppression()` (padrão `true`); `set_mic_sensitivity(u32)` (clamp 0–100), `mic_sensitivity()` (padrão 100); `mic_level_dbfs() -> f32` (atômico, `-100.0` sem sinal), `pub(crate) set_mic_level_dbfs`. `AudioProcessing::default().noise_suppression = false`.

- [ ] **Step 1: Tests** em `hub.rs`: `mic_processing_settings_clamp_and_default`. Em `audio.rs`: `capture_pipeline_gates_below_threshold` (sinal de ~-45 dBFS com sensibilidade 50 → chunks de saída zerados), `capture_pipeline_passes_everything_at_sensitivity_100`, `capture_pipeline_updates_level_before_gate` (nível reflete o sinal mesmo com gate fechado), `capture_pipeline_denoiser_bypass_when_off`. Ajustar o teste existente `capture_pipeline_downmixes_into_10ms_chunks` (novos campos do pipeline; deve continuar valendo com padrão: NS ligado muda o sinal constante, então usar `set_noise_suppression(false)` nele).
- [ ] **Step 2:** `cargo test -p media` → FAIL.
- [ ] **Step 3:** Em `audio.rs`: `CapturePipeline` ganha `denoiser`, `gate`, `was_denoising`; em `process`, depois do APM: se `hub.noise_suppression()` → `denoiser.process` (se acabou de religar, `reset()` antes); calcular `rms_dbfs` e publicar no hub; `gate.process(chunk, hub.mic_sensitivity())`; só então `push_mic` (se não mudo). Nível publicado como `-100` quando mudo.
- [ ] **Step 4:** Mudar `AudioProcessing::default()` para `noise_suppression: false` e ajustar o comentário do módulo (`audio.rs:1-3`). Verificar que `load-test` e `transport.rs` não dependem do padrão antigo.
- [ ] **Step 5:** `cargo test -p media` → PASS. Commit `feat(media): RNNoise + noise gate in capture pipeline`.

### Task 4: Comandos Tauri e contrato do backend

**Files:**
- Modify: `src-tauri/src/lib.rs` (comandos + `generate_handler!`), `ui/src/lib/types.ts` (`Backend`), `ui/src/lib/api.ts`, `ui/src/lib/mock.ts`, `ui/src/lib/api.test.ts`, `CONTRACT.md` (se existir; o teste se chama "segue o CONTRACT.md")

**Interfaces:**
- Produces: comandos `set_noise_suppression(enabled: bool)`, `set_mic_sensitivity(percent: u32)`, `mic_level() -> f32` (dBFS). `Backend.setNoiseSuppression(enabled)`, `setMicSensitivity(percent)`, `getMicLevel(): Promise<number>` (mock devolve uma senoide lenta para a UI/e2e).

- [ ] **Step 1: Test** em `api.test.ts`: adicionar `setNoiseSuppression(false)` e `setMicSensitivity(40)` à lista de chamadas esperadas (`['set_noise_suppression', { enabled: false }]`, `['set_mic_sensitivity', { percent: 40 }]`) e um caso `getMicLevel` que repassa o retorno de `mic_level`.
- [ ] **Step 2:** `pnpm --filter @sheeshcord/ui test api` → FAIL.
- [ ] **Step 3:** Implementar espelhando `set_mic_volume`. Commit `feat: backend commands for noise suppression, sensitivity and mic level`.

### Task 5: Store (estado + persistência)

**Files:**
- Modify: `ui/src/lib/room.svelte.ts`
- Test: `ui/src/lib/room.svelte.test.ts`

**Interfaces:**
- Produces: `RoomStore.noiseSuppression = $state(true)`, `micSensitivity = $state(100)`, `setNoiseSuppression(on)`, `setMicSensitivity(percent)` (clamp 0–100, arredonda); persistência em `localStorage['sheeshcord:audio']`; `start()` lê o que está salvo e reenvia ao backend (`micVolume` incluído, decisão 4).

- [ ] **Step 1: Tests**: padrões; clamp da sensibilidade; só atualiza o estado se o backend aceitou (padrão do `#run`); valores salvos são reaplicados no `start()` (backend recebe as 3 chamadas); JSON corrompido no `localStorage` cai nos padrões sem lançar.
- [ ] **Step 2:** `pnpm --filter @sheeshcord/ui test room.svelte` → FAIL.
- [ ] **Step 3:** Implementar. Commit `feat(ui): persisted noise suppression and sensitivity settings`.

### Task 6: UI em `DeviceSettings`

**Files:**
- Modify: `ui/src/components/DeviceSettings.svelte`
- Create: `ui/src/components/MedidorMicrofone.svelte` (barra de nível + marca do limiar)
- Test: `ui/src/components/DeviceSettings.test.ts` (criar se não existir)

**Interfaces:**
- Consumes: `store.noiseSuppression/micSensitivity`, `store.backend.getMicLevel()`.

- [ ] **Step 1: Tests**: chave "Supressão de ruído" (role `switch`/checkbox) reflete e altera o estado; slider "Sensibilidade do microfone: N%" chama `setMicSensitivity`; medidor tem `role="meter"` com `aria-valuenow`, e a marca do limiar some em sensibilidade 100; o polling para ao desmontar (fake timers); com `getMicLevel` rejeitando, o medidor fica em "sem sinal" sem erro na tela.
- [ ] **Step 2:** `pnpm --filter @sheeshcord/ui test DeviceSettings` → FAIL.
- [ ] **Step 3:** Implementar abaixo do slider de volume: chave, slider (0–100, passo 1; 100 = "Sem corte" no rótulo) e medidor (-60…0 dBFS; marca do limiar via `threshold_dbfs` replicado no TS, validado pela mesma tabela da Task 2). Polling a ~15 Hz num `$effect` com cleanup. Descrição acessível do slider ("mais baixo corta mais ruído de fundo").
- [ ] **Step 4:** `pnpm --filter @sheeshcord/ui lint` (inclui `svelte-check --fail-on-warnings`) → limpo. Commit `feat(ui): noise suppression toggle, sensitivity slider and live meter`.

### Task 7: Verificação final e release

- [ ] **Step 1:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --exclude load-test`, `pnpm --filter @sheeshcord/ui test` e `lint` (o CI roda tudo isso).
- [ ] **Step 2: Manual** com `pnpm tauri dev` e `SHEESHCORD_FAKE_MEDIA` desligado, entrando numa call de dois clientes: (a) teclado/ventilador com supressão ligada vs desligada; (b) sensibilidade baixando até cortar o ruído de fundo sem cortar a fala; (c) começo de frases; (d) medidor acompanha a voz e a marca do limiar bate com o corte; (e) trocar o microfone no meio da call; (f) reabrir o app e conferir que os ajustes persistem.
- [ ] **Step 3:** Atualizar o spec: marcar a seção 2 como implementada e corrigir a posição do ganho (depois do gate). Subir versão (`chore: versão 0.8.0`) seguindo o padrão dos commits de release.

## Dependências externas desta máquina

Este clone não tem toolchain: `cargo`/`rustup` e `pnpm` não estão instalados (só Node 24 e corepack). Antes da Task 1: instalar Rust stable com o target `x86_64-pc-windows-msvc` + Build Tools do Visual Studio (C++), e habilitar o pnpm (`corepack enable`, versão do `PNPM_VERSION` do CI). A primeira compilação baixa e linka o libwebrtc do LiveKit e demora.
