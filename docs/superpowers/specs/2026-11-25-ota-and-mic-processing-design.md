# Atualização OTA e processamento do microfone — design

Status: aguardando aprovação. O indicador de fala já foi corrigido (commit ae6e2d2).

## 1. Atualização OTA

**Objetivo:** o app descobre versão nova, baixa sozinho em segundo plano e o indicador pede só para reiniciar. (Escolha do usuário.)

**Não objetivos:** canal beta, rollback automático, atualização de macOS/Linux (o app é só Windows).

**Abordagem:** plugin oficial `tauri-plugin-updater` + `tauri-plugin-process` (para reiniciar). Alternativas descartadas: updater próprio (reimplementa assinatura e verificação) e só avisar com link para o GitHub (não é OTA).

**Partes**
- `src-tauri`: registrar os plugins, `bundle.createUpdaterArtifacts: true`, `plugins.updater` com `pubkey` e `endpoints` apontando para `https://github.com/HeitorMancini/microbates-call-app/releases/latest/download/latest.json`; permissões `updater:default` e `process:allow-restart` em `capabilities/default.json`.
- Backend Tauri: na abertura, e depois a cada 30 min, `check()`; se houver versão, `download()` em segundo plano e emitir o evento `update://state` (`idle | downloading{pct} | ready{version}`). `install()` + `relaunch()` só quando o usuário clica.
- UI: componente `IndicadorAtualizacao` no header de `RoomScreen.svelte`, logo após `μChat` (canto superior esquerdo). Baixando: ponto discreto com porcentagem acessível. Pronto: botão "Reiniciar para atualizar (vX.Y.Z)". Se o usuário estiver numa call, o clique pede confirmação, pois reiniciar derruba a chamada.
- Release: `release.yml` passa a assinar (`TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` como secrets) e a anexar o `.exe`, o `.sig` e o `latest.json`.
- Falhas de rede ou de verificação só vão para o log; o indicador some.

**Exige do usuário (não dá para automatizar):** gerar o par de chaves (`pnpm tauri signer generate`), colar a chave pública no `tauri.conf.json` e cadastrar a privada nos secrets do repositório. Sem isso o app não atualiza. Os instaladores já distribuídos (0.5.0) não têm o updater e precisam ser reinstalados uma vez.

**Testes:** máquina de estados do updater em Rust (injetando um cliente falso), vitest do indicador e da confirmação durante a call; o fluxo completo só se verifica manualmente com duas versões publicadas.

## 2. Supressão de ruído de qualidade e sensibilidade do microfone

**Status: implementado** (plano em `docs/superpowers/plans/2026-11-26-noise-suppression-and-sensitivity.md`). Desvios do desenho original, decididos na implementação:
- O ganho (`micVolume`) continua dentro de `MediaHub::push_mic`, então o *noise gate* roda **antes** do ganho (não depois): o limiar não depende do slider de volume e o medidor lê o sinal sem ganho.
- O gate olha 1 chunk (10 ms) à frente, sempre, para abrir antes da primeira sílaba. Custo fixo de 10 ms de latência na captura.
- Sensibilidade 100 = gate desligado; abaixo disso, limiar = `-60 + (100 - s) * 0,4` dBFS (99 → -59,6; 0 → -20). `threshold_dbfs` (Rust) e `thresholdDbfs` (TS) precisam andar juntos.
- O medidor é consultado por polling (`mic_level`, ~15×/s, só com as configurações abertas) e mostra o nível antes do gate.
- Os ajustes (`noiseSuppression`, `micSensitivity` e também `micVolume`) ficam em `localStorage` (`sheeshcord:audio`) e são reenviados ao backend na abertura.
- Limite do RNNoise medido nos testes: remove bem ruído de baixa frequência (ventilador, rumble, mais de 50 dB), mas trata um zumbido de tom puro como voz e não o remove; ruído branco sintético é atenuado de forma irregular.

**Objetivo:** voz limpa sem cortar o começo das palavras, e um controle de sensibilidade nas configurações.

**Abordagem (minha escolha):** RNNoise via crate `nnnoiseless` (Rust puro, quadros de 10 ms a 48 kHz, o formato exato do pipeline) no lugar da supressão básica do WebRTC, com um *noise gate* de sensibilidade depois dele. Alternativa descartada: DeepFilterNet, de qualidade um pouco melhor, porém muito mais pesado e com dependência de runtime de ML.

**Partes**
- `crates/media/src/denoise.rs`: `Denoiser` (envolve `nnnoiseless::DenoiseState`) e `NoiseGate` (limiar em dBFS, ataque instantâneo, release de ~250 ms, suavizado para não picotar).
- Em `audio.rs`, o chunk do microfone passa por APM (eco e AGC, sem a supressão de ruído do WebRTC) → `Denoiser` → `NoiseGate` → `push_mic`.
- Configurações persistidas e enviadas ao backend: `noiseSuppression: boolean` (padrão ligado) e `micSensitivity: 0–100` (padrão: gate aberto, ou seja, sem corte).
- UI (`DeviceSettings.svelte`): chave "Supressão de ruído" e controle deslizante "Sensibilidade do microfone" com medidor de nível ao vivo para ajustar o limiar. O mesmo limiar alimenta o indicador de fala local, para os dois concordarem.

**Testes:** `NoiseGate` (abre no primeiro chunk acima do limiar, release, sensibilidade máxima nunca corta) e `Denoiser` (silêncio permanece silêncio; ruído branco perde energia; tamanho dos quadros preservado). A qualidade percebida só se avalia ouvindo.

## 3. Volume do microfone e volume individual de cada pessoa

**Objetivo:** (a) um controle de volume do seu microfone nas configurações; (b) clicar com o botão direito no blob de alguém e ajustar o volume só dessa pessoa.

**Volume do microfone:** ganho linear aplicado no chunk do microfone em `audio.rs`, *depois* de APM, `Denoiser` e antes do `NoiseGate`/`push_mic`, com saturação em i16. Faixa 0–200% (padrão 100%). Configuração `micVolume`. UI: controle deslizante "Volume do microfone" em `DeviceSettings.svelte`, ao lado da sensibilidade, usando o mesmo medidor de nível.

**Volume por pessoa:** ganho por participante aplicado no mixer do `MediaHub` (`push_remote_audio`/mixagem por chave), faixa 0–200% (padrão 100%), vale só localmente (ninguém mais é afetado). A chave do mixer é a identidade; o áudio de tela `"{id}#screen"` segue o mesmo ganho da pessoa.
- Comando Tauri `set_participant_volume(id, volume)` e `Backend.setParticipantVolume(id, volume)` (mock incluído).
- UI: menu de contexto (`contextmenu` no blob, com `preventDefault`) com controle deslizante e botão "Restaurar 100%". Precisa ser operável por teclado (tecla de menu/Shift+F10 abre o mesmo menu) e fechar com Esc.
- Persistir por nome de pessoa em `localStorage` e reenviar ao backend quando a pessoa entra. Remover o ganho quando ela sai do mixer.

**Testes:** ganho do microfone (100% é identidade, 0% silencia, 200% satura sem estourar); ganho no mixer por pessoa (afeta só a chave certa, vale para `#screen`, é limpo em `remove_remote`); vitest do menu de contexto (abre com botão direito e pelo teclado, envia o volume, restaura 100%).

## Ordem de implementação
1. Supressão de ruído, sensibilidade e volume do microfone (mesmo trecho de `audio.rs`).
2. Volume por pessoa (hub, comando, menu de contexto).
3. OTA (já implementada).
