# Tela da call em roda — design

Data: 2026-11-25 · App: μChat (Tauri 2 + Svelte 5, pasta `ui/`)

## Intenção

A tela de login já foi redesenhada (blob do blobatar gerado pelo nome, logo μ, brilho voice-glow no botão Entrar). A tela da call ainda é a antiga: grade de iniciais coloridas, cabeçalho com servidor e status, barra de controles larga. O objetivo é que a call tenha a mesma identidade do login: **cada pessoa aparece como o blob que viu ao digitar o nome**, dispostas numa roda que se reorganiza quando alguém entra ou sai, com uma janela limpa, escura e sem moldura.

Referência visual aprovada: o mockup clicável `.superpowers/brainstorm/1889-1795578720/content/tela-call.html` (com `blobs.js` e `fala.js` ao lado). Ele está fora do git (`.superpowers/` é ignorado); abra direto no navegador ou pelo companion. Os números de geometria abaixo vêm dele.

## Objetivos

1. **Roda de blobs.** Todos os participantes num anel em volta do centro da janela, cada um com o blob do blobatar (semente = nome, igual ao login) e o nome embaixo. "Heitor (você)" para o participante local, em branco. Entrar/sair alguém faz os blobs deslizarem para as novas posições.
2. **Barra superior da janela** (34px, arrastável): "μChat" à esquerda (μ em `#FF2E6E`, "Chat" em cinza), controles minimizar/maximizar/fechar à direita. Arrastar move a janela; duplo clique maximiza (comportamento atual do `arrastavel`).
3. **Topo da call:** seletor "CALL 1 ▾" à esquerda (título grande, maiúsculas); à direita dois botões quadrados de 56px: **Configurações** (abre o diálogo de dispositivos de áudio que hoje está no botão "Dispositivos") e **Sair da call**.
4. **Rodapé:** à esquerda uma barra com **mic / fone / tela** (ícones separados por "/"; mic e fone em vermelho riscado quando desligados; tela abre o diálogo de compartilhar, ou para o compartilhamento se você já compartilha). À direita um botão de **chat** que ainda não faz nada (tooltip "Chat de texto · em breve").
5. **Compartilhamento de tela — modo palco** (padrão quando alguém começa a compartilhar): a tela ocupa o palco e os blobs deslizam e encolhem para uma fileira **na mesma linha do "CALL 1 ▾" e dos botões da direita**. Na tela: botão **Roda** e botão de tela cheia (F continua funcionando).
6. **Modo roda com tela:** o botão Roda devolve os blobs à roda (elipse mais larga) com a tela pequena no centro. Clicar na tela pequena volta ao modo palco.
7. **Vários compartilhando:** o palco mostra a última tela que você escolheu (ou a primeira, de início). Quem compartilha tem o selo **AO VIVO** acima do blob; clicar no blob de quem compartilha troca a tela. As abas de escolha atuais do `ScreenViewer` saem.
8. **Efeito de fala:** quem está falando ganha o brilho voice-glow "dither" — o mesmo visual do hover do botão Entrar — na cor de destaque do próprio blob, subindo de logo abaixo do blob (sem alargar para os lados), entre o blob e o nome.
9. **Seletor de salas:** drop-down com Call 1, Call 2 e Call 3 (a atual marcada). Escolher outra sala troca de sala **sem** a animação de sair (a janela não encolhe para o login).

## Não-objetivos

- Chat de texto (só o botão placeholder).
- Volume real de áudio por pessoa no efeito de fala (fica para depois; ver Abordagens).
- Mostrar quem está em cada sala no drop-down (exige endpoint novo no token-server).
- Mudar o servidor, a tela de login ou a coreografia de entrar/sair.
- Selo de "ensurdecido" no blob (não decidido; o selo de mic desligado continua).

## Anti-objetivos

- Blobs pulando de lugar sem animação quando a lista muda, ou o vídeo da tela religando ao trocar de modo (palco ↔ roda). A tela precisa ser **o mesmo `<video>`** nos dois modos.
- Trocar de sala e ver a janela encolher para o login e voltar.
- O efeito de fala derrubar o desempenho com 7+ pessoas.
- Perder o que hoje é acessível: lista de pessoas com papel de lista, anúncios `aria-live`, atalhos Ctrl+Shift+M/D, `prefers-reduced-motion`.

## Restrições

- Janela da sala: 1100×720 inicial, mínimo 720×520, redimensionável e maximizável (`ui/src/lib/janela.ts`). O layout tem de caber no mínimo e escalar no maximizado.
- Backend informa só **quem** fala (`Participant.speaking`), não o volume.
- `Backend` é a interface comum de Tauri e mock (`ui/src/lib/types.ts`); a UI tem de funcionar no mock (e2e roda nele).
- voice-glow é React: continua isolado em ilha carregada sob demanda (`ui/src/lib/brilho.ts`), fora do pacote inicial.
- Tema: escala de cinzas escuros (fundo da janela `#101012`), destaque vivo. Fontes do app (Bricolage).

## Abordagens consideradas

**A. Palco único com posições calculadas (escolhida).** Cada participante é um elemento persistente (chave = `id`) posicionado em absoluto por `transform`. Uma função pura `posicoes(participantes, modo, largura, altura)` devolve `{x, y, pequeno}` por pessoa para os três layouts (roda, fileira no topo, roda em volta da tela pequena); o CSS transiciona `transform` e tamanho. A tela também é um único elemento cujo retângulo muda entre palco e centro da roda. É como o mockup funciona; o deslize sai de graça e a geometria fica testável sem DOM.

**B. Layouts separados com `crossfade`/`flip` do Svelte.** Mais idiomático, mas o deslize entre containers diferentes é frágil, o `<video>` seria recriado ao trocar de modo e a lógica fica espalhada em três templates. Descartada.

**Efeito de fala — 1. Voz simulada (escolhida):** o brilho liga com `speaking`, e o `level` do VoiceBeam segue um envelope falso (sílabas de 90–220 ms com pausas), como no mockup. Nenhuma mudança em Rust. **2. Volume real:** exigiria um evento novo com o nível de áudio por participante em `crates/media` e `crates/core`. Adiado; o `level` aceita um getter, então trocar depois é local.

## Design

### Componentes

- `RoomScreen.svelte` — reescrito: barra superior, topo (seletor + ações), palco, rodapé, aviso de erro, região `aria-live`. Mantém os atalhos Ctrl+Shift+M/D.
- `Palco.svelte` (novo) — container `position: relative` medido com `ResizeObserver`; renderiza um `Blob` por participante e o `ScreenViewer` (quando há quem compartilhe), posicionando ambos a partir de `posicoes()`.
- `ui/src/lib/roda.ts` (novo) — `posicoes()` pura + constantes de geometria. Testes unitários.
- `Blob.svelte` (substitui `ParticipantTile.svelte`) — SVG do `blobatar(nome)`, nome, selo de mic desligado, selo AO VIVO, ilha do brilho de fala.
- `SeletorSala.svelte` (novo) — botão "CALL 1 ▾" + menu.
- `ControlBar.svelte` — reescrito como a barra mic/fone/tela; Configurações e Sair vão para o topo.
- `ScreenViewer.svelte` — perde o cabeçalho com abas; ganha botão Roda e recebe de fora qual participante mostrar. O resto (sessão de vídeo, estados, tela cheia, F, stats) fica.
- `ControlesJanela.svelte` — passa a viver dentro da barra superior na sala (no login continua como hoje).
- `ParticipantList.svelte` — some ou vira só a semântica (`<ul aria-label="Pessoas na sala">`) dentro do `Palco`.

### Geometria (referência 1100×720; escalar proporcionalmente à área útil)

- Blob normal 96px (imagem do blobatar a 130% do corpo, centrada), nome 17px, gap blob→nome 20px.
- **Roda:** centro do palco (`cy` 14px acima do centro geométrico). 1 pessoa: no centro. 2 pessoas: lado a lado na horizontal (±120px). 3+: elipse com `rx = 110 + 22n`, `ry = 90 + 18n`, começando **à esquerda** (ângulo π) e seguindo no sentido horário na tela (esquerda → topo → direita → baixo), passo `2π/n`. Esses valores valem para 1100×720; escalar pela área útil e limitar para não encostar no topo nem no rodapé.
- **Ordem na roda (decisão minha):** você primeiro (fica à esquerda), depois os outros **na ordem em que apareceram** durante a sessão nesta sala. A UI guarda a lista de ids já vistos; quem sai é removido; quem entra vai para o fim. O backend não garante ordem estável entre eventos, por isso a ordem não pode vir da lista recebida, senão os blobs trocariam de lugar a cada atualização. A mesma ordem vale para a fileira do palco.
- **Palco:** fileira centrada na linha do topo (centro vertical do "CALL 1 ▾", y≈60 dentro da janela), blobs de 46px, nome 12px, gap 10px, passo 84px; não pode encostar no seletor nem nos botões da direita. Tela em `{x: 40, y: 128, w: W−80, h: ~452}`, acima do rodapé.
- **Roda com tela:** elipse rx≈330, ry≈222; tela pequena 340×192 no centro.
- Transição: 650 ms `cubic-bezier(.2,.8,.2,1)` em `transform`, tamanho do corpo e fonte do nome. Com `prefers-reduced-motion`, sem transição.

### Estado de visualização (local da tela, não vai para o store)

- `modo: 'palco' | 'roda'` — vira `'palco'` quando o número de quem compartilha passa de 0 para 1; botão Roda põe `'roda'`; clicar na tela pequena põe `'palco'`. Sem ninguém compartilhando, só existe a roda.
- `assistindo: id | null` — quem está no palco; clicar num blob com AO VIVO troca; se quem você assistia para, cai para o primeiro que ainda compartilha.

### Efeito de fala

- Um `VoiceBeam` por blob, mesmos parâmetros do `brilho.ts` (`type: 'pill'`, `look: 'dither'`, `theme: 'dark'`, `idle: 0`, `pixelMagnet: 0`, `scale: 1.2`, `reach: 0.8`, `spread: 1.3`, `pixelSize: 1`), `active = speaking`, `level` = getter de um envelope falso compartilhado.
- Cores: `destaqueDe(corDoBlob(svg))` → `[tom(d,.08,.9), d.cor, tom(d,−.08,1), tom(d,.15,.6)]` (como o `BotaoEntrar`).
- Host do brilho: absoluto dentro do corpo, `left/right: −22%`, `top: −4%`, `bottom: −16%`, atrás da imagem do blob.
- `brilho.ts` ganha opção de `level` dinâmico (hoje fixo em 0.6), sem mudar o botão Entrar.

### Trocar de sala

- `RoomStore` guarda os últimos `JoinParams` (nome e senha) e ganha `trocarSala(sala)`: marca `trocando = true`, faz `leave()` e `join({...ultimo, room: sala})`, e desmarca no fim (sucesso ou erro).
- **Estado levado para a nova sala (decisão minha):** mic desligado e fone desligado continuam como estavam. `trocarSala` reaplica `setMuted(true)`/`setDeafened(true)` logo após entrar, porque o backend entra sempre com o mic aberto e abrir o mic de quem estava mudo é um problema de privacidade. O compartilhamento de tela **para** ao trocar, e um anúncio `aria-live` avisa ("Compartilhamento encerrado ao trocar de sala.").
- `App.svelte` não dispara a coreografia de sair enquanto `trocando` for verdadeiro (a janela fica no tamanho da sala; pode haver um fade curto do palco).
- **Se a nova sala falhar (decisão minha):** a tela da call continua aberta, com a roda vazia, e o aviso de erro mostra o motivo. O seletor continua funcionando para tentar outra sala (ou a mesma de novo), e **Sair** volta ao login normalmente. Para isso o `App.svelte` precisa de uma condição "falha de troca": ela mantém a fase `sala` enquanto houver erro vindo de `trocarSala`. Essa condição termina quando outra troca dá certo ou quando o usuário sai. O seletor marca a sala que falhou como a atual, porque é o `roomName`; a falha fica visível no aviso.
- Salas: constante `SALAS = ['Call 1', 'Call 2', 'Call 3']` ao lado de `DEFAULT_ROOM` em `types.ts`.

### O que some da tela atual

Nome do servidor, ponto de status e contagem "N pessoas, M falando" do cabeçalho; abas do `ScreenViewer`; rótulos de texto nos botões de controle (viram ícones com `aria-label`/`title`).

## Deixado para quem implementa

- Fórmula exata do raio/escala da roda e do passo da fileira para caber de 1 a ~12 pessoas entre 720×520 e tela cheia (a partir de ~12, encolher os blobs).
- Onde mostrar "Reconectando…"/"Sem conexão" agora que o cabeçalho de status saiu (sugestão: ao lado de "μChat" na barra superior, só quando não estiver conectado).
- Montar a ilha do voice-glow de cada blob sob demanda (na primeira fala) para não criar N raízes React de saída.
- Ajustar testes unitários (`ParticipantList.test.ts`, `ControlBar.test.ts`, `ScreenViewer.svelte.test.ts`) e o e2e (`ui/e2e/app.spec.ts`: seletores por texto "Mudo"/"Sair"/"Conectado", heading "Sua tela", `.anel` no teste de reduced-motion) para o novo layout, mantendo a cobertura dos mesmos fluxos.
