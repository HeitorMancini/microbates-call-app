# load-test

Cliente headless que simula uma sala do Sheeshcord no LiveKit usando o SDK Rust
oficial (`livekit` 0.10, libwebrtc nativo):

- **N participantes** (padrão 7) entram na mesma sala, todos no mesmo processo;
- o participante `publisher` publica uma **tela sintética** (barras de cor +
  quadrado em movimento + faixa de ruído, I420) como `Screenshare`, 1080p30 e
  bitrate configurável (padrão 3000 kbps, VP8, sem simulcast);
- os outros N‑1 (`viewer-1`…) assinam a tela e medem, numa janela após o aquecimento:
  - **tempo de conexão** (`Room::connect`) e tempo até o primeiro quadro;
  - **bitrate recebido** (`inbound-rtp.bytesReceived`);
  - **FPS recebido** (quadros entregues ao sink nativo);
  - **perda** (`packetsLost` / (`packetsReceived` + `packetsLost`)) e **jitter** (`inbound-rtp.jitter`).

A saída é um resumo legível no stdout e um JSON (`--output arquivo.json`, ou no stdout).
O processo termina com código **0** se aprovado, **1** se algum limite foi violado
e **2** em erro de execução.

## Uso

```powershell
# servidor local (veja infra/README.md) ou o do host no Radmin
cargo run -p load-test --release -- --url ws://127.0.0.1:7880 -n 7 --output load-report.json

# 720p com 4 participantes, H.264, limites mais rígidos
cargo run -p load-test --release -- -n 4 --width 1280 --height 720 --codec h264 `
  --bitrate-kbps 2500 --min-fps 28 --max-loss-pct 1 --duration-secs 60
```

Credenciais: `--api-key/--api-secret` ou `LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET`
(padrão: chaves de desenvolvimento do `infra/livekit.yaml`). `--url` ou `LIVEKIT_URL`.

| Limite             | Opção                | Padrão |
| ------------------ | -------------------- | ------ |
| bitrate mínimo     | `--min-bitrate-kbps` | 1000   |
| FPS mínimo         | `--min-fps`          | 24     |
| perda máxima (%)   | `--max-loss-pct`     | 2      |
| jitter máximo (ms) | `--max-jitter-ms`    | 30     |
| conexão máxima (ms)| `--max-connect-ms`   | 3000   |

Além disso, todos os espectadores precisam estar recebendo vídeo (`viewers_receiving`).
Os limites são aplicados ao **pior** espectador.

> Rode em `--release`: em debug a geração/encode de 1080p30 + N‑1 decoders no
> mesmo processo pode limitar o FPS pela CPU, não pela rede.

## Simulação de perda/latência

A ferramenta **não instala nem controla** emuladores de rede diretamente. Ela expõe
um gancho (`ImpairmentHook`): após todos receberem a tela, executa `--impair-cmd`
e, ao final, `--restore-cmd`, com os valores pedidos nas variáveis
`LT_LOSS_PCT`, `LT_LATENCY_MS`, `LT_JITTER_MS`. Sem `--impair-cmd`, as opções
`--loss-pct/--latency-ms/--jitter-ms` só imprimem os comandos sugeridos.

### Windows: clumsy

[clumsy](https://jagt.github.io/clumsy/) (WinDivert, precisa de administrador).
Filtre só a faixa UDP de mídia do LiveKit (50000–50100) para não afetar o resto:

```powershell
cargo run -p load-test --release -- -n 7 --loss-pct 2 --latency-ms 40 `
  --impair-cmd 'start "" "C:\tools\clumsy\clumsy.exe" --filter "udp and udp.DstPort >= 50000 and udp.DstPort <= 50100" --drop on --drop-chance %LT_LOSS_PCT% --lag on --lag-time %LT_LATENCY_MS%' `
  --restore-cmd 'taskkill /IM clumsy.exe /F'
```

(O clumsy também pode ser iniciado manualmente na interface gráfica antes do teste.)

### Linux: tc netem

```bash
cargo run -p load-test --release -- -n 7 --loss-pct 2 --latency-ms 40 --jitter-ms 10 \
  --impair-cmd 'sudo tc qdisc replace dev lo root netem delay ${LT_LATENCY_MS}ms ${LT_JITTER_MS}ms loss ${LT_LOSS_PCT}%' \
  --restore-cmd 'sudo tc qdisc del dev lo root'
```

Para testes via Radmin, aplique a degradação na interface do Radmin VPN em vez de `lo`.

## Testes

```powershell
cargo test -p load-test
```

- unitários: cálculo de bitrate/FPS/perda, agregação, limites, padrão de teste e gancho;
- `tests/smoke.rs`: 3 clientes (360p15, 4 s) contra um `livekit-server` local
  iniciado pelo próprio teste. **É pulado** (com aviso `SKIPPED`) se o binário não
  estiver em `LIVEKIT_SERVER_BIN`, no `PATH` ou em `%USERPROFILE%\.local\bin`.

## Limitações

- Todos os clientes rodam no mesmo processo/máquina: mede a capacidade do SFU e
  do link local, não o upload real do host. Para medir o Radmin, rode a ferramenta
  em outra máquina da VPN apontando para `ws://26.x.x.x:7880`.
- Só vídeo (tela); não publica áudio.
- Jitter é a estimativa instantânea do WebRTC (maior valor entre início e fim da janela).
