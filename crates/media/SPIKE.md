# Spike: mídia real (crates/media)

## Resultado: LiveKit Rust SDK funciona no Windows (plano B não foi necessário)

- `livekit = 0.10.1` / `libwebrtc 0.4.0` / `webrtc-sys 0.3.53` compilam e linkam com MSVC
  (VS 2022 Build Tools 14.44) usando o libwebrtc pré-compilado baixado pelo `webrtc-sys`.
- Única exigência: **CRT estático**. Sem ele o link falha com
  `LNK2038: 'RuntimeLibrary': valor 'MT_StaticRelease' não corresponde ao valor 'MD_DynamicRelease'`
  (libwebrtc é compilado com `/MT`). Corrigido em `.cargo/config.toml`
  (`-C target-feature=+crt-static` para `*-pc-windows-msvc`). Isso vale para o workspace
  inteiro (inclusive o app Tauri) e força rebuild completo na primeira vez.
- Primeiro build do crate: ~20 min (máquina compartilhada com outro build); incrementais são rápidos.
- Evidência: `cargo test -p media` → teste `tests/livekit.rs` com `livekit-server` local,
  dois `LiveKitTransport` com fontes fake: Alice publica tom 440 Hz + barras de cor (720p30, VP8),
  Bob recebe (exemplo real: `remote_audio_samples: 14880`, `remote_video_frames ≥ 1`,
  prévia RGBA recebida, `ScreenShareChanged{sharing:true}` e participantes visíveis), ~7 s.

## Arquitetura

As traits do core são independentes entre si, então a mídia bruta passa por um `MediaHub`
compartilhado:

| Origem | Caminho |
| --- | --- |
| microfone (`CpalAudio`/`FakeAudio`) | `hub.push_mic` (10 ms, 48 kHz mono) → `NativeAudioSource` → Opus (libwebrtc) |
| tela (`WgcScreen`/`FakeScreen`) | BGRA → `hub.push_screen_bgra` → I420 (libyuv), escala p/ preset → `NativeVideoSource` |
| áudio remoto | `NativeAudioStream` (48 kHz mono) → mixer por participante (40 ms de colchão, máx. 300 ms) → alto-falante |
| vídeo remoto/local | `NativeVideoStream` → prévia RGBA → `room://screen-frame` |

- **Áudio**: `cpal` (WASAPI) numa thread dedicada; downmix + reamostragem linear para 48 kHz;
  processamento com o **APM do WebRTC** (o mesmo do libwebrtc do SDK): AEC, supressão de
  ruído, AGC e filtro passa-alta, com o sinal de playback alimentando `process_reverse_stream`.
  Supressão de ruído é, portanto, nativa (sem feature opcional). Opus encode/decode é do libwebrtc.
- **Tela**: `windows-capture` 2 (Windows.Graphics.Capture), ids `monitor:<n>` (1-based) e
  `window:<HWND>`; frames limitados ao fps do preset; presets 1080p60 (6 Mbps), 1080p30 (4 Mbps),
  720p30 (2,5 Mbps); `simulcast: false`, codec **VP8** por padrão (`TransportOptions` permite H264).
- **Fakes** (feature `fake`): `FakeAudio` (seno 440 Hz, "alto-falante" que drena o mixer em
  tempo real) e `FakeScreen` (barras de cor rolando, 640x360). O app usa-os com
  `SHEESHCORD_FAKE_MEDIA=1` (transporte e token-server continuam reais).

## Decisão: vídeo para o WebView via `room://screen-frame` (JPEG base64)

Para o spike, cada quadro (remoto e a prévia local) é convertido para RGBA, reduzido para
**largura ≤ 1280**, limitado a **15 fps por participante**, codificado em **JPEG q=70**
(`jpeg-encoder`, em `spawn_blocking`) e emitido em base64 no evento
`room://screen-frame { participantId, width, height, jpegBase64 }`.
A conversão só acontece se houver alguém inscrito nas prévias (o app sempre se inscreve).

Motivo: zero mudança no contrato/UI e funciona em qualquer WebView. Custo: ~100–200 KB por
quadro em base64 a 1280x720, CPU de encode/decode e serialização IPC; não serve para 1080p60
na tela. Próximo passo sugerido: janela nativa/wgpu ou canal binário (Tauri `Channel` /
protocolo customizado) com frames brutos.

## Riscos e limitações conhecidos

- **AEC**: o APM recebe o sinal de referência, mas o atraso fixo (`stream_delay_ms = 60`) não é
  medido; com caixas de som pode haver eco residual. Só validado com testes unitários/fakes,
  não com hardware real. Alternativa: `PlatformAudio` do SDK (ADM do WebRTC com AEC integrado).
- **Encode por hardware**: VP8 é software. H264 com `VideoEncoderBackend::Auto` pode usar
  NVENC/QSV/AMF dependendo da build do libwebrtc — não testado; 1080p60 em VP8 software pode
  custar bastante CPU em jogos.
- **Captura**: WGC exige Windows 10 1903+; jogos em tela cheia exclusiva podem não ser capturados;
  borda amarela desativada (`WithoutBorder`) pode ser ignorada em versões antigas do Windows.
- **Reamostragem linear** (qualidade aceitável para voz; trocar por `rubato` se necessário).
- **Debug builds** são lentos para conversão de vídeo; use `--release` para medir desempenho.
- O CRT estático é global ao workspace; dependências C/C++ novas precisam respeitar `/MT`.
