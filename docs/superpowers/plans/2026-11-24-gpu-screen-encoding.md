# Codificação de tela na GPU (NVIDIA + AMD) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** O compartilhamento de tela do Sheeshcord passa a ser codificado na GPU (NVENC na NVIDIA, AMF na AMD, QSV na Intel) via Media Foundation, com fallback automático para o encoder de software atual.

**Architecture:** O módulo `crates/media/src/hwenc.rs` (já commitado em `600371f`, ainda desligado) abre um MFT de hardware assíncrono, recebe NV12 e entrega bitstream H.264/AV1 a um `NativeVideoSource::new_encoded`, publicado com `VideoEncoderBackend::PreEncoded` (pass-through do libwebrtc, que repassa PLI como pedido de keyframe e o BWE como pedido de bitrate). `transport.rs` sonda o hardware antes de publicar; se nada funcionar, ou se o encoder falhar no meio da transmissão, republica no caminho de software (`ScreenOutput::Source`).

**Tech Stack:** Rust, crate `windows` 0.62 (Media Foundation), livekit 0.10.1 / libwebrtc 0.4.0 (pre-encoded), Tauri 2.

**Spec:** Pedido do usuário nesta sessão: "o objetivo era rodar em hardware, usando GPU para codificação" + "planeje o swap do uso de CPU no codec para a GPU, pensando em compatibilidade tanto para GPUs nvidia quanto amd". Contexto técnico: `crates/media/SPIKE.md` (nota de hardware encode) e o commit `600371f`.

## Global Constraints

- Builds no Windows: `$env:CARGO_TARGET_DIR="C:\t"` (MAX_PATH do webrtc-sys). `cargo test --workspace` precisa de `-j 2` (erro 1455, page file).
- Testes de integração: `$env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"`.
- Ordem de codec: AV1 (hardware) → H.264 (hardware) → software AV1 → H.264 → VP8. Não muda `TransportOptions::screen_codec`/`fallback_codec`.
- Matriz de suporte esperada: NVIDIA H.264 GTX 600+ / AV1 só RTX 40+; AMD H.264 GCN+ / AV1 só RX 7000+ (RDNA3/4); Intel H.264 quase todas / AV1 Arc e Meteor Lake+. O probe decide, nada é hardcoded por modelo.
- Keyframe periódico a cada `GOP_SECONDS = 4`; bitrate entre `MIN_BITRATE_BPS = 500_000` e `preset_max_bitrate(preset)` (9/6/3 Mbps).
- `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --all` limpos ao fim de cada task. Os `#[allow(dead_code)]` provisórios em `lib.rs` (`mod hwenc`) e `hub.rs` (`ScreenOutput::Feed`) saem na Task 2.
- Máquinas sem GPU / sem encoder (CI): testes de GPU imprimem `SKIPPED` e passam; nunca falham por falta de hardware.
- Commits com trailer `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`.

## Review Focus

1. **NVIDIA consumer com limite de sessões NVENC** (GeForce: até 8 simultâneas; OBS/Discord abertos consomem): o MFT do probe tem que ser destruído antes de `HwScreenEncoder::start`, e falha ao abrir deve cair para software, não derrubar o share. Teste na Task 1 (`probe_releases_the_encoder_before_returning`).
2. **Captura de janela com dimensões ímpares** (ex.: 1365×767): NVENC e AMF rejeitam NV12 ímpar. Esperado: frame recortado para par, share continua. Teste na Task 1 (`odd_frames_are_cropped_to_even`).
3. **Notebook híbrido Intel iGPU + NVIDIA/AMD dGPU**: `MFTEnumEx` pode listar o QSV primeiro. Esperado: preferir o encoder discreto, com o iGPU como alternativa. Teste na Task 1 (`encoders_prefer_discrete_vendors`).
4. **Driver reset / encoder morre no meio do share** (troca de driver, TDR, sessão NVENC tomada): esperado voltar ao software em poucos segundos sem o espectador ficar em "Aguardando a imagem…" para sempre. Teste na Task 3 (`hardware_failure_falls_back_to_software`).
5. **Sem AV1 no hardware (RTX 30, RX 6000)**: o probe tem que escolher H.264 de hardware em vez de AV1 em software (que trava 1080p60). Teste na Task 2 (`hardware_candidates_keep_preference_order`) + validação manual na Task 4.

---

### Task 1: Endurecer `hwenc` para NVIDIA, AMD e Intel

**Files:**
- Modify: `crates/media/src/hwenc.rs` (`Nv12Frame::from_i420` ~48, `encoders` ~221, `Mft::open` ~328, `probe_codec` ~655, testes ~803 e ~882)

**Interfaces:**
- Consumes: nada novo.
- Produces:
  - `pub(crate) enum GpuVendor { Nvidia, Amd, Intel, Other }` com `fn from_vendor_id(id: &str) -> GpuVendor` (`"VEN_10DE"`→Nvidia, `"VEN_1002"`→Amd, `"VEN_8086"`→Intel, case-insensitive, aceita a string completa do atributo `MFT_ENUM_HARDWARE_VENDOR_ID_Attribute`) e `fn rank(self) -> u8` (Nvidia 0, Amd 0, Intel 1, Other 2).
  - `probe(...) -> Option<HwProbe>` com `pub(crate) struct HwProbe { pub codec: VideoCodec, pub encoder: String, pub vendor: GpuVendor }` (substitui a tupla `(VideoCodec, String)`).
  - `Nv12Frame::from_i420` passa a produzir sempre `width` e `height` pares (recorta 1 px à direita/embaixo).

- [ ] **Step 1: Escrever os testes puros que falham** (no `mod tests` comum, ~882)

```rust
#[test]
fn vendor_ids_map_to_gpu_vendors() {
    assert_eq!(GpuVendor::from_vendor_id("VEN_10DE"), GpuVendor::Nvidia);
    assert_eq!(GpuVendor::from_vendor_id("ven_1002&DEV_7590"), GpuVendor::Amd);
    assert_eq!(GpuVendor::from_vendor_id("VEN_8086"), GpuVendor::Intel);
    assert_eq!(GpuVendor::from_vendor_id(""), GpuVendor::Other);
}

#[test]
fn encoders_prefer_discrete_vendors() {
    let mut list = vec![(GpuVendor::Intel, "qsv"), (GpuVendor::Nvidia, "nvenc"), (GpuVendor::Other, "x")];
    list.sort_by_key(|(v, _)| v.rank());
    assert_eq!(list.iter().map(|(_, n)| *n).collect::<Vec<_>>(), ["nvenc", "qsv", "x"]);
}

#[test]
fn odd_frames_are_cropped_to_even() {
    let buffer = I420Buffer::new(1365, 767);
    let frame = Nv12Frame::from_i420(&buffer, 0);
    assert_eq!((frame.width, frame.height), (1364, 766));
    assert_eq!(frame.data.len(), 1364 * 766 * 3 / 2);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `$env:CARGO_TARGET_DIR="C:\t"; cargo test -p media --lib hwenc`
Expected: erro de compilação "cannot find type `GpuVendor`" e, após criar o tipo, `odd_frames_are_cropped_to_even` falhando com `(1365, 767)`.

- [ ] **Step 3: Implementar**
  - `GpuVendor` + `from_vendor_id` + `rank` em `hwenc.rs` (fora de `imp`, para testar em qualquer SO).
  - `Nv12Frame::from_i420`: `w = width & !1`, `h = height & !1`; converte só a região recortada (`yuv_helper::i420_to_nv12` com os mesmos strides de origem e dimensões `w`,`h`).
  - `encoders(codec)`: ler `MFT_ENUM_HARDWARE_VENDOR_ID_Attribute` (`GetAllocatedString`) de cada `IMFActivate`, e ordenar estável por `GpuVendor::rank` mantendo o mérito do `SORTANDFILTER` dentro de cada rank. Retornar `Vec<(IMFActivate, GpuVendor)>`; `open_first` loga `vendor`.
  - `Mft::open`: se `CODECAPI_AVEncCommonRateControlMode` falhou antes dos media types, tentar de novo depois de `set_input_type` (NVIDIA aceita só depois dos tipos em alguns drivers). Mesmo para `CODECAPI_AVEncMPVGOPSize`.
  - `probe_codec`: rejeitar o encoder (erro `MF_E_INVALIDMEDIATYPE`) se o primeiro frame não for `key`, ou se for H.264 sem NAL 7 (SPS) depois de `with_h264_parameter_sets`. O `Mft` do probe é dropado antes de `probe` retornar.

- [ ] **Step 4: Estender o teste de GPU** (`imp::tests::hardware_encoders_produce_decodable_keyframes`, ~809) e adicionar `probe_releases_the_encoder_before_returning`

```rust
// no teste existente, para cada codec que abriu:
assert!(frames[0].key, "{name}: first output must be a keyframe");
if codec == VideoCodec::H264 {
    assert!(h264_nal_types(&frames[0].payload).any(|t| t == H264_NAL_SPS), "{name}: keyframe without SPS");
}
eprintln!("{codec:?} via {name} ({vendor:?}): {} bytes", frames[0].payload.len());

#[test]
fn probe_releases_the_encoder_before_returning() {
    // Abrir e soltar 12 vezes seguidas excede o limite de 8 sessões NVENC se houver vazamento.
    let cfg = HwConfig { fps: 30, max_bitrate_bps: 2_000_000 };
    let Some(first) = probe(&[VideoCodec::H264], 640, 360, cfg) else {
        eprintln!("SKIPPED: no hardware H.264 encoder");
        return;
    };
    for _ in 0..12 {
        assert_eq!(probe(&[VideoCodec::H264], 640, 360, cfg).map(|p| p.encoder), Some(first.encoder.clone()));
    }
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `$env:CARGO_TARGET_DIR="C:\t"; cargo test -p media --lib hwenc -- --nocapture`
Expected: todos `ok`; nesta máquina (RX 9060 XT) imprime `H264 via AMDh264Encoder (Amd)` e `AV1 via AMDav1Encoder (Amd)`. Em CI sem GPU: `SKIPPED`.

- [ ] **Step 6: Commit**

```bash
git add crates/media/src/hwenc.rs
git commit -m "feat(media): vendor-aware hardware encoder probe (NVIDIA/AMD/Intel)"
```

---

### Task 2: Publicar a tela pelo encoder de GPU

**Files:**
- Modify: `crates/media/src/transport.rs` (`TransportOptions` ~37-60, `screen_publish_options` ~88, `publish_screen_track` ~105, `ScreenTrack` ~144, `publish_screen` ~366, `ScreenJob` ~430)
- Modify: `crates/media/src/lib.rs` (remover `#[allow(dead_code, unused_imports)]` de `mod hwenc`; reexportar `ScreenEncoderInfo`)
- Modify: `crates/media/src/hub.rs` (remover `#[allow(dead_code)]` de `ScreenOutput::Feed`)
- Test: `crates/media/src/transport.rs` (mod tests), `crates/media/tests/livekit.rs`

**Interfaces:**
- Consumes: `hwenc::{probe, HwProbe, HwConfig, HwScreenEncoder, hardware_codecs}` (Task 1); `HwScreenEncoder::start(codec, cfg, NativeVideoSource) -> HwScreenEncoder`, `.feed() -> ScreenFeed`, `.failed() -> bool`.
- Produces:
  - `TransportOptions.hardware_encoder: bool` (default `true`).
  - `fn screen_publish_options(preset, codec, backend: VideoEncoderBackend) -> TrackPublishOptions`.
  - `fn hardware_candidates(options: &TransportOptions) -> Vec<VideoCodec>`: vazio se `!options.hardware_encoder`, senão `hardware_codecs(&[screen_codec, fallback_codec])`.
  - `pub struct ScreenEncoderInfo { pub codec: VideoCodec, pub encoder: String, pub hardware: bool }` e `pub async fn LiveKitTransport::screen_encoder(&self) -> Option<ScreenEncoderInfo>` (lê do `ScreenTrack` ativo).
  - `enum ScreenEncoding { Software { source: NativeVideoSource }, Hardware { source: NativeVideoSource, encoder: HwScreenEncoder, cfg: HwConfig } }` guardado em `ScreenJob` (mantém o encoder vivo pelo share).

- [ ] **Step 1: Testes unitários que falham** (mod tests de `transport.rs`)

```rust
#[test]
fn hardware_candidates_keep_preference_order() {
    let on = TransportOptions::default();
    assert_eq!(hardware_candidates(&on), vec![VideoCodec::AV1, VideoCodec::H264]);
    let vp8_first = TransportOptions { screen_codec: VideoCodec::VP8, ..on };
    assert_eq!(hardware_candidates(&vp8_first), vec![VideoCodec::H264]);
    let off = TransportOptions { hardware_encoder: false, ..on };
    assert!(hardware_candidates(&off).is_empty());
}
```

- [ ] **Step 2: Teste de integração que falha** (`tests/livekit.rs`): `join` ganha parâmetro `hardware_encoder: bool`; o teste existente usa `false`, e um novo `screen_share_uses_gpu_encoder_when_available` usa `true`:

```rust
// depois de wait_for("remote video", ...):
let info = alice.transport.screen_encoder().await.expect("screen encoder info");
eprintln!("screen encoder: {info:?}");
if std::env::var_os("MEDIA_EXPECT_HW").is_some() {
    assert!(info.hardware, "expected a GPU encoder, got {info:?}");
}
// e no teste com hardware_encoder=false:
assert!(!alice.transport.screen_encoder().await.unwrap().hardware);
```

Run: `$env:CARGO_TARGET_DIR="C:\t"; $env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"; cargo test -p media`
Expected: FAIL (campo `hardware_encoder` e `screen_encoder()` inexistentes).

- [ ] **Step 3: Implementar em `publish_screen`**
  - `let candidates = hardware_candidates(&self.options);` se não vazio: `tokio::task::spawn_blocking(move || hwenc::probe(&candidates, width, height, cfg))` com `cfg = HwConfig { fps: preset.fps(), max_bitrate_bps: preset_max_bitrate(preset) as u32 }`.
  - Com `Some(HwProbe { codec, encoder, vendor })`: `source = NativeVideoSource::new_encoded(VideoResolution { width, height })`, publicar só com `[codec]` e `VideoEncoderBackend::PreEncoded`, `HwScreenEncoder::start(codec, cfg, source.clone())`, `hub.set_screen_sink(Some((ScreenOutput::Feed(enc.feed()), width, height)))`, log `info!(codec, encoder, ?vendor, "screen encoding on the GPU")`.
  - Se probe `None` ou publish falhar: caminho de software atual, intacto (`VideoEncoderBackend::Auto`, `screen_codec_order`, `ScreenOutput::Source`), log `info!("screen encoding in software")`.
  - `report_screen_encoder`: no caminho de hardware o `encoder_implementation` é "LiveKit pre-encoded passthrough"; logar também o nome do MFT (de `ScreenEncoderInfo`) e nunca contar `QualityLimitationReason::Cpu` para troca de codec.

- [ ] **Step 4: Rodar e ver passar**

Run: `$env:CARGO_TARGET_DIR="C:\t"; $env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"; $env:MEDIA_EXPECT_HW="1"; cargo test -p media -- --nocapture`
Expected: todos `ok`; log `screen encoder: ScreenEncoderInfo { codec: AV1, encoder: "AMDav1Encoder", hardware: true }`. Depois `cargo clippy --workspace --all-targets -j 2 -- -D warnings` sem saída de erro.

- [ ] **Step 5: Commit**

```bash
git add crates/media/src crates/media/tests/livekit.rs
git commit -m "feat(media): publish screen share from the GPU encoder"
```

---

### Task 3: Fallback para software quando a GPU falha no meio do share

**Files:**
- Modify: `crates/media/src/transport.rs` (`maintain_screen` ~445, `switch_screen_codec` ~523)
- Modify: `crates/media/src/hwenc.rs` (gancho de teste em `HwScreenEncoder`)
- Test: mod tests de `transport.rs`, `crates/media/tests/livekit.rs`

**Interfaces:**
- Consumes: `ScreenEncoding`, `ScreenEncoderInfo`, `HwScreenEncoder::failed()` (Task 2).
- Produces:
  - `enum MaintainAction { Continue, FallBackToSoftware, SwitchCodec }` e `fn next_action(hw_failed: bool, cpu_limited_checks: u32, has_fallback: bool, hardware: bool) -> MaintainAction` (puro).
  - `async fn republish_screen_software(job: &mut ScreenJob, codecs: &[VideoCodec]) -> bool` (generaliza `switch_screen_codec`: cria `NativeVideoSource::new(res, true)`, publica com `Auto`, troca o sink do hub para `ScreenOutput::Source`, atualiza `ScreenEncoderInfo { hardware: false, .. }`).
  - `pub fn LiveKitTransport::fail_hardware_encoder_for_test(&self)` atrás de `#[doc(hidden)]` (marca `failed` no `HwScreenEncoder` ativo; em `HwScreenEncoder`, `pub(crate) fn mark_failed(&self)`).

- [ ] **Step 1: Testes que falham**

```rust
#[test]
fn maintenance_decisions() {
    use MaintainAction::*;
    assert_eq!(next_action(true, 0, false, true), FallBackToSoftware);
    assert_eq!(next_action(false, CPU_LIMITED_CHECKS, true, true), Continue); // GPU: nunca troca por CPU
    assert_eq!(next_action(false, CPU_LIMITED_CHECKS, true, false), if SWITCH_ON_CPU_LIMIT { SwitchCodec } else { Continue });
    assert_eq!(next_action(false, 0, true, false), Continue);
}
```

Integração (`tests/livekit.rs`), `hardware_failure_falls_back_to_software`: share com `hardware_encoder: true`; se `screen_encoder().hardware` for `false`, imprimir `SKIPPED` e sair; senão `fail_hardware_encoder_for_test()`, guardar `remote_video_frames` atual e `wait_for("video after fallback", 15s, || frames > antes + 30)`; por fim `assert!(!screen_encoder().await.unwrap().hardware)`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `$env:CARGO_TARGET_DIR="C:\t"; $env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"; cargo test -p media maintenance hardware_failure`
Expected: FAIL (símbolos inexistentes).

- [ ] **Step 3: Implementar** `next_action` e o uso dele em `maintain_screen`: checar `hw.failed()` a cada tick de 250 ms (não só a cada 5 s de stats). Em `FallBackToSoftware`, `republish_screen_software(job, &screen_codec_order(...))`. `switch_screen_codec` vira um caso de `republish_screen_software`.

- [ ] **Step 4: Rodar e ver passar** (mesmo comando, com `-- --nocapture`)
Expected: `ok`; log `hardware screen encoder failed` seguido de `screen encoding in software`.

- [ ] **Step 5: Commit**

```bash
git add crates/media/src crates/media/tests/livekit.rs
git commit -m "feat(media): fall back to software when the GPU encoder fails mid-share"
```

---

### Task 4: Chave de desligar no app, docs e validação em NVIDIA e AMD

**Files:**
- Modify: `src-tauri/src/lib.rs:64` (montagem de `TransportOptions`)
- Modify: `README.md` (~142, seção de codecs/desempenho), `crates/media/SPIKE.md` (~59), doc comment de `TransportOptions` em `crates/media/src/transport.rs`
- Test: `src-tauri/src/lib.rs` (mod tests), `crates/media/tests/livekit.rs`

**Interfaces:**
- Consumes: `TransportOptions.hardware_encoder` (Task 2), `screen_encoder()` (Task 2).
- Produces: `fn hardware_encoder_enabled(value: Option<&str>) -> bool` (falso só para `"0"`, `"off"`, `"false"`, case-insensitive), lido de `SHEESHCORD_HW_ENCODER`.

- [ ] **Step 1: Teste que falha**

```rust
#[test]
fn hardware_encoder_env_switch() {
    assert!(hardware_encoder_enabled(None));
    assert!(hardware_encoder_enabled(Some("1")));
    for off in ["0", "off", "FALSE"] {
        assert!(!hardware_encoder_enabled(Some(off)));
    }
}
```

Run: `$env:CARGO_TARGET_DIR="C:\t"; cargo test -p sheeshcord-app hardware_encoder_env`
Expected: FAIL (função inexistente).

- [ ] **Step 2: Implementar** e usar em `TransportOptions { hardware_encoder: hardware_encoder_enabled(std::env::var("SHEESHCORD_HW_ENCODER").ok().as_deref()), ..Default::default() }`. Rodar de novo, ver PASS.

- [ ] **Step 3: Teste de desempenho `#[ignore]`** `screen_share_sustains_1080p60_on_gpu` em `tests/livekit.rs`: share `ScreenPreset::P1080Fps60` com `hardware_encoder: true`; `SKIPPED` se `!info.hardware`; medir `remote_video_frames` de Bob por 10 s após o primeiro frame; `assert!(fps >= 50.0, "{info:?}: {fps:.1} fps")`.

Run: `$env:CARGO_TARGET_DIR="C:\t"; $env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"; cargo test -p media --test livekit -- --ignored --nocapture`
Expected nesta máquina (AMD): PASS com ≥ 50 fps (no software era ~23 fps a 1080p30).

- [ ] **Step 4: Validação manual por fabricante** (registrar no README uma tabela "GPU / codec escolhido / fps 1080p60"):
  - AMD (esta máquina, RX 9060 XT): esperado AV1 `AMDav1Encoder`.
  - NVIDIA RTX 40: esperado AV1 (`NVIDIA AV1 Encoder MFT`). NVIDIA RTX 20/30 ou GTX: esperado H.264 (`NVIDIA H.264 Encoder MFT`).
  - Em cada uma: rodar o teste `--ignored` acima (ou o app com `RUST_LOG=media=info`) e conferir o log `screen encoding on the GPU`. No app real, 1 compartilhador + 2 espectadores; no console dos espectadores (`video.ts`), `freezeCount` estável e fps ≈ 60.
  - `SHEESHCORD_HW_ENCODER=0` força software (log `screen encoding in software`).

- [ ] **Step 5: Docs**: README (~142) e SPIKE.md (~59) descrevem codificação via Media Foundation (AMF/NVENC/QSV), a ordem AV1 HW → H.264 HW → software, a matriz de suporte das Global Constraints, `SHEESHCORD_HW_ENCODER` e os números medidos. O doc comment de `TransportOptions` deixa de dizer que no Windows tudo roda em software.

- [ ] **Step 6: Verificação final e commit**

Run: `$env:CARGO_TARGET_DIR="C:\t"; $env:LIVEKIT_SERVER_BIN="C:\t\lk\livekit-server.exe"; cargo fmt --all; cargo clippy --workspace --all-targets -j 2 -- -D warnings; cargo test --workspace -j 2`
Expected: clippy sem erros; todos `test result: ok`.

```bash
git add src-tauri/src/lib.rs README.md crates/media/SPIKE.md crates/media/tests/livekit.rs crates/media/src/transport.rs
git commit -m "feat: GPU screen encoding toggle, docs and 1080p60 check"
```

---

## Riscos conhecidos (fora do escopo, documentar)

- BGRA→I420→NV12 continua na CPU (barato perto do encode); upload D3D11 zero-copy fica para depois.
- Sem simulcast no pre-encoded: todos os espectadores decodificam o mesmo codec. O WebView2 decodifica AV1 (dav1d em software se a GPU do espectador não tiver AV1), aceitável em 1080p60 em CPUs modernas.
- O pass-through anuncia H.264 `42e01f` (nível 3.1) mesmo em 1080p60; o Chromium segue o SPS, mas vale observar em espectadores NVIDIA/AMD na validação manual.
