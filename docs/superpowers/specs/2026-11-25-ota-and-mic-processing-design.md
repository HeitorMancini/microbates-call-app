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

**Objetivo:** voz limpa sem cortar o começo das palavras, e um controle de sensibilidade nas configurações.

**Abordagem (minha escolha):** RNNoise via crate `nnnoiseless` (Rust puro, quadros de 10 ms a 48 kHz, o formato exato do pipeline) no lugar da supressão básica do WebRTC, com um *noise gate* de sensibilidade depois dele. Alternativa descartada: DeepFilterNet, de qualidade um pouco melhor, porém muito mais pesado e com dependência de runtime de ML.

**Partes**
- `crates/media/src/denoise.rs`: `Denoiser` (envolve `nnnoiseless::DenoiseState`) e `NoiseGate` (limiar em dBFS, ataque instantâneo, release de ~250 ms, suavizado para não picotar).
- Em `audio.rs`, o chunk do microfone passa por APM (eco e AGC, sem a supressão de ruído do WebRTC) → `Denoiser` → `NoiseGate` → `push_mic`.
- Configurações persistidas e enviadas ao backend: `noiseSuppression: boolean` (padrão ligado) e `micSensitivity: 0–100` (padrão: gate aberto, ou seja, sem corte).
- UI (`DeviceSettings.svelte`): chave "Supressão de ruído" e controle deslizante "Sensibilidade do microfone" com medidor de nível ao vivo para ajustar o limiar. O mesmo limiar alimenta o indicador de fala local, para os dois concordarem.

**Testes:** `NoiseGate` (abre no primeiro chunk acima do limiar, release, sensibilidade máxima nunca corta) e `Denoiser` (silêncio permanece silêncio; ruído branco perde energia; tamanho dos quadros preservado). A qualidade percebida só se avalia ouvindo.

## Ordem de implementação
1. Supressão de ruído e sensibilidade (afeta só o backend de áudio e as configurações).
2. OTA (mexe em CI e exige as chaves do usuário).
