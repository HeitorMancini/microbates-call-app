# Áudio no compartilhamento de tela — design

## Intenção
Hoje o compartilhamento de tela publica só vídeo. Quem assiste deve ouvir o áudio que corresponde à origem compartilhada:
- **Janela:** só o áudio do app dono da janela.
- **Monitor:** o áudio de todo o sistema.
- Em ambos os casos, **o áudio do próprio microchat fica de fora** (evita eco: as vozes dos outros voltariam para eles).

## Metas / não-metas
- Meta: áudio sincronizado o bastante com o vídeo para uso comum (jogo, vídeo, apresentação), sem eco.
- Não-meta: AEC ou redução de ruído no áudio da tela; seleção de dispositivo de áudio; macOS/Linux (o app é Windows-only no WGC).
- Anti-meta: qualquer cenário em que a voz de um participante retorne a ele pelo áudio da tela.

## Restrições
- Loopback por processo do WASAPI (`AUDIOCLIENT_ACTIVATION_PARAMS` com `PROCESS_LOOPBACK`) exige Windows 10 2004+; o WGC exige 1903. Abaixo disso, o share segue sem áudio e a UI avisa.
- Capturas de áudio do WASAPI são não-`Send`: rodam em thread própria, como `CpalAudio`.

## Abordagens consideradas
- **A (escolhida):** loopback por processo para os dois casos. Janela = modo *include* com o PID do app (e filhos); monitor = modo *exclude* com o PID do microchat. Um mecanismo só, sem mixagem própria. Menos código e menor custo.
- B: loopback comum para monitor + por processo para janela. Não exclui o próprio app, causa eco. Descartada.

## Design
1. **core (`traits.rs`)**: `ScreenSource::start` passa a receber a opção de áudio (`with_audio: bool`); `Transport::publish_screen` recebe o mesmo flag. A origem da janela expõe o PID (`ScreenSourceInfo`).
2. **media: `screen_audio.rs` (novo)**: abre o cliente de loopback por processo, converte para PCM mono i16 a 48 kHz (mesmo formato do mic) e entrega via `MediaHub::push_screen_audio`.
3. **hub**: novo sink de áudio de tela, análogo ao `ScreenSink`, que encaminha amostras para um `NativeAudioSource`.
4. **transport**: em `publish_screen`, se `with_audio`, publica uma segunda faixa com `TrackSource::ScreenshareAudio`; `unpublish_screen` a remove. Falha ao publicar o áudio não derruba o vídeo (log + aviso).
5. **receptor**: o mixer passa a ser chaveado por faixa (identidade + sid), não só por participante, para que o volume da tela seja independente do da voz. `read_audio` recebe a chave.
6. **tauri / UI**: comando de start recebe `withAudio`; a UI mostra o botão "Compartilhar áudio" (ligado por padrão) no seletor de origem, e anuncia (`announce`) quando o áudio não estiver disponível.
7. **Ciclo de vida:** se o app da janela fechar, o áudio para junto com o vídeo; ao parar o share, a captura é encerrada no `Drop`.

## Decisões minhas (revisar)
- Áudio ligado por padrão, com opção de desligar.
- Sem processamento de voz no áudio da tela, só resample para 48 kHz.
- Falha de áudio não impede o vídeo.

## Fica para o implementador
Escolha da crate (`windows` direto vs. wrapper `wasapi`), tamanho dos buffers/latência, API exata do mock/fake para testes, e estratégia de teste (fake de loopback; teste manual de eco com duas contas).
