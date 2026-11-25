# Tela da call em roda — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Substituir a tela da call atual (grade de iniciais) pela roda de blobs do blobatar com palco de tela compartilhada, seletor de salas, barra superior arrastável e o brilho voice-glow de fala.

**Architecture:** Um `Palco` em tela inteira posiciona cada blob e a tela por `transform`, a partir de uma função pura `posicoes()` (`ui/src/lib/roda.ts`); o CSS anima as mudanças. Troca de sala vive no `RoomStore` (`trocarSala`) e o `App` passa a seguir `store.presente` em vez de `store.inRoom`, para não encolher a janela durante a troca.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest + Testing Library (jsdom), Playwright (e2e no mock), blobatar 2.7.0, voice-glow 0.5.0 (ilha React).

**Spec:** `docs/superpowers/specs/2026-11-25-tela-call-roda-design.md`. Mockup de referência: `.superpowers/brainstorm/1889-1795578720/content/tela-call.html`.

## Global Constraints

- Tudo em `ui/`; comandos rodam de `ui/` com `pnpm`.
- Textos da UI em português; nomes de código em português, como no resto do projeto.
- Janela da sala: 1100×720 inicial, mínimo 720×520 (`ui/src/lib/janela.ts`). Geometria de referência a 1100×720.
- Fundo da janela `#101012`; μ do "μChat" em `#FF2E6E`; fonte Bricolage.
- Transições de layout: 650 ms `cubic-bezier(.2,.8,.2,1)`; com `prefers-reduced-motion: reduce`, sem transição e sem voice-glow.
- voice-glow só via `import('../lib/brilho')` dinâmico (fora do pacote inicial).
- Salas: `SALAS = ['Call 1', 'Call 2', 'Call 3']` em `ui/src/lib/types.ts`.
- Nada muda em Rust, no mock (exceto se um teste exigir), no login ou na coreografia de entrar/sair.
- Arquivos com "μ": salvar em UTF-8; mensagens de commit com acento via `git commit -F <arquivo UTF-8>` e trailer `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`.

## Review Focus

1. **Ordem recebida embaralhada:** o backend manda a mesma lista em outra ordem → ninguém muda de lugar. Teste em Task 1 (`ordemDeChegada`).
2. **Janela mínima com muita gente:** 12 pessoas em 720×520 → todo blob da roda fica entre y=120 e y=400, e entre x=60 e x=660. Teste em Task 1.
3. **Clique duplo / troca durante troca:** segunda `trocarSala` enquanto a primeira roda → devolve `false` sem mexer em nada; escolher a sala atual → no-op. Testes em Task 2.
4. **Quem você assiste para de compartilhar:** o palco cai para quem ainda compartilha; quando o último para, volta à roda sem tela, e o próximo compartilhamento abre em modo palco de novo. Teste em Task 4.
5. **Ensurdecido ao trocar de sala:** chega na nova sala ensurdecido *e* mudo. Teste em Task 2.

---

### Task 1: Geometria e ordem da roda

**Files:**
- Create: `ui/src/lib/roda.ts`
- Test: `ui/src/lib/roda.test.ts`

**Interfaces:**
- Produces:
  - `type Modo = 'palco' | 'roda'`
  - `interface Ponto { x: number; y: number; pequeno: boolean }` (centro do corpo do blob, coordenadas da janela)
  - `interface Retangulo { x: number; y: number; w: number; h: number }`
  - `interface Layout { blobs: Map<string, Ponto>; tela: Retangulo | null }`
  - `posicoes(ids: string[], o: { modo: Modo; temTela: boolean; largura: number; altura: number }): Layout`
  - `ordemDeChegada(anterior: string[], atuais: Participant[]): string[]`

- [ ] **Step 1: Write the failing tests** (`describe('posicoes')` e `describe('ordemDeChegada')`)

```ts
const L = 1100, A = 720;
it('1 pessoa fica no centro', () => {
  expect(posicoes(['a'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs.get('a'))
    .toEqual({ x: 550, y: 346, pequeno: false });
});
it('2 pessoas lado a lado, a primeira à esquerda', () => {
  const b = posicoes(['a', 'b'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs;
  expect(b.get('a')).toMatchObject({ x: 430, y: 346 });
  expect(b.get('b')).toMatchObject({ x: 670, y: 346 });
});
it('3+ começam à esquerda e seguem pelo topo (horário)', () => {
  const b = posicoes(['a', 'b', 'c', 'd'], { modo: 'roda', temTela: false, largura: L, altura: A }).blobs;
  expect(b.get('a')!.x).toBeCloseTo(550 - (110 + 22 * 4));
  expect(b.get('b')!.y).toBeLessThan(346); // topo
  expect(b.get('b')!.x).toBeCloseTo(550);
});
it('palco: fileira pequena em y=60, passo 84, centrada; tela 40,128', () => {
  const r = posicoes(['a', 'b', 'c'], { modo: 'palco', temTela: true, largura: L, altura: A });
  expect([...r.blobs.values()].map((p) => [p.x, p.y, p.pequeno])).toEqual([[466, 60, true], [550, 60, true], [634, 60, true]]);
  expect(r.tela).toEqual({ x: 40, y: 128, w: 1020, h: 452 });
});
it('roda com tela: elipse 330×222 e tela 340×192 centrada', () => {
  const r = posicoes(['a', 'b'], { modo: 'roda', temTela: true, largura: L, altura: A });
  expect(r.blobs.get('a')).toMatchObject({ x: 220, y: 346 });
  expect(r.tela).toEqual({ x: 380, y: 250, w: 340, h: 192 });
});
it('sem tela, tela é null mesmo no modo palco', () => {
  expect(posicoes(['a'], { modo: 'palco', temTela: false, largura: L, altura: A }).tela).toBeNull();
});
it('12 pessoas em 720×520 cabem entre o topo e o rodapé', () => {
  const ids = Array.from({ length: 12 }, (_, i) => `p${i}`);
  for (const p of posicoes(ids, { modo: 'roda', temTela: false, largura: 720, altura: 520 }).blobs.values()) {
    expect(p.y).toBeGreaterThanOrEqual(120); expect(p.y).toBeLessThanOrEqual(400);
    expect(p.x).toBeGreaterThanOrEqual(60); expect(p.x).toBeLessThanOrEqual(660);
  }
});
it('fileira do palco não invade seletor nem botões (720 de largura, 12 pessoas)', () => {
  const ids = Array.from({ length: 12 }, (_, i) => `p${i}`);
  const xs = [...posicoes(ids, { modo: 'palco', temTela: true, largura: 720, altura: 520 }).blobs.values()].map((p) => p.x);
  expect(Math.min(...xs)).toBeGreaterThanOrEqual(210); expect(Math.max(...xs)).toBeLessThanOrEqual(720 - 170);
});
// ordemDeChegada (p = helper de Participant como em ParticipantList.test.ts)
it('local primeiro, depois na ordem recebida', () => {
  expect(ordemDeChegada([], [p({ id: 'b' }), p({ id: 'eu', isLocal: true }), p({ id: 'c' })])).toEqual(['eu', 'b', 'c']);
});
it('lista embaralhada não muda a ordem', () => {
  expect(ordemDeChegada(['eu', 'b', 'c'], [p({ id: 'c' }), p({ id: 'b' }), p({ id: 'eu', isLocal: true })])).toEqual(['eu', 'b', 'c']);
});
it('quem sai some, quem entra vai para o fim', () => {
  expect(ordemDeChegada(['eu', 'b', 'c'], [p({ id: 'd' }), p({ id: 'eu', isLocal: true }), p({ id: 'c' })])).toEqual(['eu', 'c', 'd']);
});
```

- [ ] **Step 2: Run** `pnpm test -- src/lib/roda.test.ts` → FAIL (módulo não existe).

- [ ] **Step 3: Implement `roda.ts`.** Valores de referência (1100×720), escalados por `s = min(largura/1100, altura/720)`:
  - centro `cx = largura/2`, `cy = altura/2 − 14`.
  - roda sem tela: 1 → centro; 2 → `cx ± 120·s`, `cy`; n≥3 → `rx = (110 + 22n)·s`, `ry = (90 + 18n)·s`, ângulo `π + i·2π/n`, `x = cx + rx·cos`, `y = cy + ry·sin`. Depois limitar: `ry ≤ (altura − 280)/2`, `rx ≤ (largura − 120)/2` (o centro de cima e o de baixo da janela ficam livres, porque o seletor, os botões e o rodapé estão nos cantos).
  - roda com tela: `rx = 330·s`, `ry = 222·s` (mesmos limites), mesmos ângulos (2 pessoas também na elipse); tela `340·s × 192·s` centrada em `(cx, cy)`.
  - palco: `y = 60` fixo, `pequeno: true`, passo `min(84, (largura − 420)/(n−1))`, centrada em `cx`; quando o passo fica menor que 46 os blobs se sobrepõem como uma pilha de avatares (aceito só em janelas estreitas com muita gente); tela `{x: 40, y: 128, w: largura − 80, h: altura − 268}`.
  - arredondar coordenadas para inteiros.

- [ ] **Step 4: Run** `pnpm test -- src/lib/roda.test.ts` → PASS.
- [ ] **Step 5: Commit** `feat(ui): geometria e ordem da roda de blobs`.

---

### Task 2: Troca de sala no store

**Files:**
- Modify: `ui/src/lib/types.ts` (adicionar `SALAS`), `ui/src/lib/room.svelte.ts`, `ui/src/App.svelte:27-29`
- Test: `ui/src/lib/room.svelte.test.ts`

**Interfaces:**
- Produces (em `RoomStore`):
  - `trocando: boolean` ($state), `falhaTroca: boolean` ($state)
  - `presente: boolean` ($derived `inRoom || trocando || falhaTroca`) — o que o `App` usa para decidir entre sala e login.
  - `trocarSala(sala: string): Promise<boolean>`
  - `SALAS: readonly string[]` em `types.ts`.

- [ ] **Step 1: Write the failing tests** (usar `makeStore()` e `join` do arquivo, com `room: 'Call 1'` onde indicado)

```ts
it('troca de sala sem sair da call e mantém nome e senha', async () => {
  const store = makeStore();
  await store.join({ ...join, room: 'Call 1', password: 's' });
  const visto: boolean[] = [];
  const parar = $effect.root(() => { $effect(() => { visto.push(store.presente); }); });
  expect(await store.trocarSala('Call 2')).toBe(true);
  parar();
  expect(store.roomName).toBe('Call 2');
  expect(store.local?.name).toBe('Ana');
  expect(visto.every(Boolean)).toBe(true);
  expect(store.trocando).toBe(false);
});
it('chega mudo na nova sala se estava mudo', async () => { /* toggleMute; trocarSala; expect(store.muted).toBe(true) */ });
it('chega ensurdecido e mudo se estava ensurdecido', async () => { /* toggleDeafen; trocarSala; expect deafened && muted */ });
it('compartilhamento para ao trocar e é anunciado', async () => {
  /* startShare; trocarSala; expect(store.sharing).toBe(false);
     expect(store.announcement).toMatch(/Compartilhamento encerrado ao trocar de sala/) */
});
it('falha na troca mantém a call aberta com o erro; sair limpa', async () => {
  const store = makeStore();
  await store.join({ ...join, room: 'Call 1' });
  expect(await store.trocarSala('   ')).toBe(false);
  expect(store.falhaTroca).toBe(true);
  expect(store.presente).toBe(true);
  expect(store.error).not.toBeNull();
  expect(await store.trocarSala('Call 3')).toBe(true);
  expect(store.falhaTroca).toBe(false);
  await store.trocarSala('   ');
  await store.leave();
  expect(store.falhaTroca).toBe(false);
  expect(store.presente).toBe(false);
});
it('escolher a sala atual não faz nada', async () => { /* trocarSala('Call 1') → true; status continua 'connected' sem leave */ });
it('troca durante troca é ignorada', async () => {
  /* const a = store.trocarSala('Call 2'); const b = store.trocarSala('Call 3');
     expect(await b).toBe(false); expect(await a).toBe(true); expect(store.roomName).toBe('Call 2') */
});
```

O arquivo já é `.svelte.test.ts`, então `$effect.root` funciona nele.

- [ ] **Step 2: Run** `pnpm test -- src/lib/room.svelte.test.ts` → FAIL.

- [ ] **Step 3: Implement.**
  - `join()` guarda os `JoinParams` normalizados em `#ultimo`.
  - `trocarSala(sala)`: se `trocando` → `false`; se `sala === roomName && !falhaTroca` → `true`; senão guarda `muted`, `deafened`, `sharing`; `trocando = true`; chama `backend.leaveRoom()` direto (sem o anúncio "Você saiu da sala."), depois `join({ ...#ultimo, room: sala })`. Sucesso: `falhaTroca = false`; se estava ensurdecido → `setDeafened(true)` e `deafened = true`; senão se estava mudo → `setMuted(true)`; se compartilhava → anuncia `Compartilhamento encerrado ao trocar de sala.`. Falha → `falhaTroca = true`. Sempre `trocando = false` no fim (`finally`).
  - `leave()` zera `falhaTroca`.
  - `App.svelte`: o `$effect` da coreografia lê `store.presente` em vez de `store.inRoom`.

- [ ] **Step 4: Run** `pnpm test -- src/lib/room.svelte.test.ts` → PASS (todos, incluindo os antigos).
- [ ] **Step 5: Commit** `feat(ui): trocar de sala sem sair da call`.

---

### Task 3: Blob com brilho de fala

**Files:**
- Create: `ui/src/components/Blob.svelte`, `ui/src/lib/voz.ts`
- Modify: `ui/src/lib/brilho.ts`
- Test: `ui/src/components/Blob.test.ts`, `ui/src/lib/voz.test.ts`

**Interfaces:**
- Produces:
  - `montarBrilho(alvo: HTMLElement, opcoes?: { nivel?: () => number }): Brilho` — `level` = `opcoes.nivel ?? 0.6` (o `BotaoEntrar` não muda).
  - `criarEnvelope(aleatorio?: () => number): (t: number) => number` em `voz.ts` — nível de voz simulado em [0.05, 1] a partir do tempo em ms.
  - `Blob.svelte` props: `{ participant: Participant; pequeno: boolean; assistindo: boolean; onassistir?: () => void; nivel: () => number }`. Raiz é `<li data-testid="participante" data-speaking data-muted>`.

- [ ] **Step 1: Write the failing tests**

```ts
// voz.test.ts
it('fica entre 0.05 e 1 e varia com o tempo', () => {
  const nivel = criarEnvelope();
  const vals = Array.from({ length: 200 }, (_, i) => nivel(i * 16));
  vals.forEach((v) => { expect(v).toBeGreaterThanOrEqual(0.05); expect(v).toBeLessThanOrEqual(1); });
  expect(new Set(vals.map((v) => v.toFixed(2))).size).toBeGreaterThan(5);
});
it('sorteio baixo vira pausa', () => { expect(criarEnvelope(() => 0)(500)).toBeCloseTo(0.05); });

// Blob.test.ts — vi.mock('../lib/brilho', () => ({ montarBrilho: vi.fn(() => ({ atualizar: vi.fn(), destruir: vi.fn() })) }))
it('leitor de tela ouve nome e estado (mesmo texto de antes)', () => {
  /* render Blob { participant: p({ name: 'Ana', isLocal: true, speaking: true }) ... };
     spokenText(listitem) === 'Ana: você, falando' (helper copiado de ParticipantList.test.ts) */
});
it('mostra o blob do blobatar do nome', () => {
  /* img src começa com 'data:image/svg+xml' e contém encodeURIComponent(blobatar('Ana')).slice(0, 40) */
});
it('quem compartilha tem AO VIVO clicável', async () => {
  /* sharingScreen: true, onassistir = vi.fn(); click button { name: 'Assistir a tela de Bia' };
     expect(onassistir).toHaveBeenCalled(); aria-pressed reflete `assistindo` */
});
it('sem compartilhar não há AO VIVO', () => { /* queryByRole('button') === null */ });
it('só carrega o brilho quando a pessoa fala', async () => {
  /* render speaking: false → montarBrilho não chamado; rerender speaking: true → await tick(); chamado 1 vez;
     atualizar chamado com (true, array de 4 cores) */
});
it('mic desligado mostra selo', () => { /* data-muted="true" e selo presente */ });
```

- [ ] **Step 2: Run** `pnpm test -- src/lib/voz.test.ts src/components/Blob.test.ts` → FAIL.

- [ ] **Step 3: Implement.**
  - `voz.ts`: alvos novos a cada 90–220 ms (20% de chance de pausa 0.05; senão 0.45–1.0), interpolando ~60 ms entre o alvo anterior e o atual; a função é pura em relação a `t` mais o estado interno do envelope.
  - `Blob.svelte`: SVG de `blobatar(participant.name)` como `<img alt="">` data URI, corpo 96px (46px se `pequeno`), imagem a 130% centrada; nome embaixo (17px / 12px, gap 20px / 10px; `max-width` com reticências); "(você)" `aria-hidden` e branco para o local; `sr-only` com a mesma descrição do `ParticipantTile` atual; selo de mic desligado; AO VIVO como `<button aria-label="Assistir a tela de {nome}" aria-pressed={assistindo}>` (fundo `#ff2e4d`, 9px, acima do blob). Brilho: host `<span class="brilho">` com `left/right: −22%`, `top: −4%`, `bottom: −16%`, atrás da imagem; na primeira vez que `speaking` fica `true` (e sem `prefers-reduced-motion`), `import('../lib/brilho')` → `montarBrilho(host, { nivel })`; depois `atualizar(speaking, cores)` com as cores de `destaqueDe(corDoBlob(svg))` como no `BotaoEntrar.svelte:17-19`; `destruir()` no unmount. Com movimento reduzido: sem voice-glow, só um contorno estático na cor do blob enquanto fala.
  - `brilho.ts`: parâmetro `opcoes` conforme Interfaces.

- [ ] **Step 4: Run** `pnpm test -- src/lib/voz.test.ts src/components/Blob.test.ts` → PASS.
- [ ] **Step 5: Commit** `feat(ui): blob do participante com brilho de fala`.

---

### Task 4: Palco (roda, fileira e tela)

**Files:**
- Create: `ui/src/components/Palco.svelte`
- Modify: `ui/src/components/ScreenViewer.svelte`
- Delete: `ui/src/components/ParticipantTile.svelte`, `ui/src/components/ParticipantList.svelte`, `ui/src/components/ParticipantList.test.ts`
- Test: `ui/src/components/Palco.test.ts`, `ui/src/components/ScreenViewer.svelte.test.ts`

**Interfaces:**
- Consumes: `posicoes`, `ordemDeChegada`, `Modo` (Task 1); `Blob` e `criarEnvelope` (Task 3).
- Produces:
  - `Palco.svelte` props `{ participants: Participant[]; backend: Backend }`.
  - `ScreenViewer.svelte` props `{ backend: Backend; sharer: Participant; pequena: boolean; onroda: () => void }` (troca `sharers` por `sharer`; abas removidas).

- [ ] **Step 1: Write the failing tests** (`Palco.test.ts`; backend = `createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false })`; `ResizeObserver` falso no teste reportando 1100×720)

```ts
it('lista as pessoas com rótulo e contagem', () => { /* getByRole('list', { name: 'Pessoas na sala (3)' }) com 3 listitems */ });
it('sem ninguém mostra o aviso', () => { /* 'Ninguém por aqui ainda. Chame a turma para a sala.' */ });
it('posiciona cada blob pela roda', () => {
  /* 2 pessoas → style transform do 1º listitem contém translate com x=430 (centro − metade do item) */
});
it('alguém compartilha → modo palco; Roda → tela pequena; clicar na tela → palco', async () => {
  /* rerender com Bia sharingScreen; tela (testid 'palco-tela') sem classe 'pequena'; blobs com data-pequeno="true";
     click button 'Roda' → tela com classe 'pequena', blobs data-pequeno="false";
     click na tela → sem 'pequena' */
});
it('clicar no AO VIVO troca a tela assistida', async () => {
  /* Bia e Caio compartilhando; heading 'Tela de Bia'; click 'Assistir a tela de Caio' → heading 'Tela de Caio' */
});
it('quem você assiste para → cai para quem ainda compartilha; último para → roda sem tela; novo compartilhamento abre no palco', async () => {
  /* assistindo Caio; Caio para → heading 'Tela de Bia'; Bia para → queryByTestId('palco-tela') null;
     clicou Roda antes? garantir: após zerar, Gui compartilha → tela sem 'pequena' */
});
```

Em `ScreenViewer.svelte.test.ts`: trocar `sharers: [...]` por `sharer: ..., pequena: false, onroda: () => {}`; manter os dois testes; adicionar `it('botão Roda chama onroda e some na tela pequena')`.

- [ ] **Step 2: Run** `pnpm test -- src/components/Palco.test.ts src/components/ScreenViewer.svelte.test.ts` → FAIL.

- [ ] **Step 3: Implement.**
  - `Palco.svelte`: `position: absolute; inset: 0` (cobre a janela; coordenadas de `posicoes` são da janela). Tamanho por `ResizeObserver`. Estado local: `ordem` (atualizada com `ordemDeChegada` a cada mudança de `participants`), `modo` (`'palco'` quando o número de quem compartilha vai de 0 para ≥1), `assistindo` (id; se sumir dos compartilhadores, o primeiro que restar). Um envelope `criarEnvelope()` compartilhado; `nivel = () => envelope(performance.now())` para todos os blobs. `<ul aria-label="Pessoas na sala ({n})">` com `{#each ordem as id (id)}` → `Blob` num wrapper com `style:transform="translate(x − 60px, y − metade do corpo)"` e `data-pequeno`. A tela fica num único `<div data-testid="palco-tela" class:pequena>` posicionado por `layout.tela`, renderizado enquanto houver quem compartilhe (assim o `ScreenViewer` e o `<video>` não são recriados ao trocar de modo); clique nela com `pequena` → `modo = 'palco'` (ignorar cliques em botões). Transições conforme Global Constraints. O `Palco` não captura cliques nas áreas vazias (`pointer-events: none` no container; `auto` nos blobs e na tela), para a barra/topo/rodapé continuarem clicáveis.
  - `ScreenViewer.svelte`: recebe `sharer`; remove `chosenId`, abas e o `<header>` visual — o `<h2>` (`Sua tela` / `Tela de {nome}`) fica como `sr-only`; botão `Roda` (ícone de 4 pontos + texto "Roda") ao lado do tela cheia, no canto superior direito, só quando `!pequena`; com `pequena`, esconder controles e stats. Resto (sessão, estados, F, tela cheia, stats) igual.

- [ ] **Step 4: Run** `pnpm test` → PASS (suíte toda; `ParticipantList.test.ts` removido).
- [ ] **Step 5: Commit** `feat(ui): palco com roda, fileira e tela compartilhada`.

---

### Task 5: Moldura da call (barra, topo, rodapé, seletor)

**Files:**
- Create: `ui/src/components/SeletorSala.svelte`
- Modify: `ui/src/components/RoomScreen.svelte` (reescrito), `ui/src/components/ControlBar.svelte` (reescrito), `ui/src/components/Icon.svelte` (ícones novos: `settings`, `phone-leave`, `chat`, `chevron-down`, `roda` — caminhos SVG do mockup)
- Test: `ui/src/components/ControlBar.test.ts`, `ui/src/components/SeletorSala.test.ts`, `ui/src/components/RoomScreen.test.ts`

**Interfaces:**
- Consumes: `Palco` (Task 4); `store.trocarSala`, `store.trocando`, `SALAS` (Task 2).
- Produces: `SeletorSala.svelte` props `{ atual: string; ocupado: boolean; onescolher: (sala: string) => void }`.

- [ ] **Step 1: Write the failing tests**

```ts
// ControlBar.test.ts — manter os testes de Mudo, Ensurdecer e Compartilhar (nomes acessíveis iguais:
// 'Mudo', 'Ensurdecer', 'Compartilhar tela', 'Parar de compartilhar'); remover 'Dispositivos' e 'Sair' daqui.
// SeletorSala.test.ts
it('abre o menu com as 3 salas e a atual marcada', async () => {
  /* click button 'Call 1' (aria-haspopup="menu", aria-expanded vira "true");
     menuitemradio 'Call 1' aria-checked="true"; 'Call 2' e 'Call 3' "false" */
});
it('escolher chama onescolher e fecha', async () => { /* click 'Call 2' → onescolher('Call 2'); menu some */ });
it('Escape fecha e devolve o foco ao botão', async () => {});
it('ocupado desabilita o botão', () => {});
// RoomScreen.test.ts (store do mock já na sala, autoSpeak: false)
it('título é a sala atual', () => { /* getByRole('heading', { level: 1, name: 'Call 1' }) */ });
it('Configurações abre Dispositivos de áudio', async () => { /* click 'Configurações' → dialog 'Dispositivos de áudio' */ });
it('Sair da call desconecta', async () => { /* click 'Sair da call' → store.status 'disconnected' */ });
it('trocar no seletor muda de sala', async () => { /* click 'Call 1' → 'Call 2' → heading 'Call 2' */ });
it('chat está marcado como em breve', () => { /* button 'Chat de texto (em breve)' com aria-disabled="true" */ });
it('mostra status só quando não está conectado', () => { /* conectado: queryByText('Reconectando…') null */ });
```

- [ ] **Step 2: Run** `pnpm test -- src/components` → FAIL.

- [ ] **Step 3: Implement.**
  - `RoomScreen.svelte`: `Palco` por baixo de tudo; por cima, em camadas:
    - **barra superior** (34px, `use:arrastavel={{ maximizarComDuploClique: true }}`): à esquerda `<span><b>μ</b>Chat</span>` (13px, 700, cinza `#8a8a92`, μ `#FF2E6E`) e, quando `status !== 'connected'`, o rótulo do `STATUS_LABEL` atual; à direita espaço livre para o `ControlesJanela`, que continua no `App` (fixo em top 10px / right 12px, já alinhado à barra).
    - **topo** (top 44px, left/right 40px): `<h1>` contendo o `SeletorSala` (34px, 800, maiúsculas); à direita dois botões quadrados de 56px (raio 14px, fundo `#1d1d21`, borda `#2f2f35`): `aria-label="Configurações"` (abre o `Dialog` de `DeviceSettings` que hoje está no `ControlBar`) e `aria-label="Sair da call"` (`store.leave()`; ícone fica vermelho no hover).
    - **rodapé** (bottom 36px, left/right 40px): `ControlBar` à esquerda; à direita botão de chat 76px `aria-label="Chat de texto (em breve)"` `aria-disabled="true"` `title="Chat de texto · em breve"`.
    - aviso de erro e região `aria-live` como hoje; atalhos Ctrl+Shift+M/D iguais.
    - `onescolher` → `store.trocarSala(sala)`; `ocupado = store.trocando`.
  - `ControlBar.svelte`: barra arredondada com mic / fone / tela (ícones 28px, separadores "/" `aria-hidden`); `aria-label` = 'Mudo' / 'Ensurdecer' / 'Compartilhar tela' ou 'Parar de compartilhar', `aria-pressed` nos dois primeiros, `title` com o atalho; mic e fone desligados em vermelho. Mantém o `Dialog` de compartilhar.
  - `SeletorSala.svelte`: botão com o nome + chevron (gira 180° aberto); menu `role="menu"` com `menuitemradio` para cada `SALAS`; fecha com Escape, clique fora ou escolha; animação de entrada do mockup (`opacity` + `translateY(-6px) scale(.98)`, 180 ms).

- [ ] **Step 4: Run** `pnpm test` e `pnpm lint` → ambos sem erros.
- [ ] **Step 5: Commit** `feat(ui): barra superior, seletor de salas e novos controles da call`.

---

### Task 6: e2e e verificação na janela real

**Files:**
- Modify: `ui/e2e/app.spec.ts`

- [ ] **Step 1: Atualizar o e2e** para o novo layout, mantendo os fluxos:
  - `fluxo completo`: lista `/Pessoas na sala/` com 5 itens e Ana primeiro; trocar `getByText('Conectado')` por `expect(page.getByText('Reconectando…')).toHaveCount(0)`; `Mudo` igual; compartilhar igual; heading `Sua tela` continua (sr-only); `Sair` → `Sair da call`.
  - novo `test('troca de sala mantém o mudo')`: entrar, `Mudo`, abrir `Call 1`, escolher `Call 2` → heading `Call 2`, botão `Mudo` com `aria-pressed="true"`, botão `Entrar` nunca visível.
  - `visualizador`: igual (heading `Tela de Bia Rocha`, tela cheia).
  - `prefers-reduced-motion`: trocar o seletor `.anel` por `[data-testid="participante"] .brilho` e verificar que não há `canvas` dentro e que o wrapper do blob tem `transition-duration` `0s`.
- [ ] **Step 2: Run** e2e no Edge (Chromium do Playwright não está instalado na máquina): criar `ui/pw-edge.config.ts` com `{...base, projects: [{ name: 'edge', use: { ...base.projects[0].use, channel: 'msedge' } }]}`, rodar `pnpm exec playwright test -c pw-edge.config.ts`, esperar todos passando, apagar a config e `test-results`.
- [ ] **Step 3: Verificar na janela real:** da raiz do repo, `$env:CARGO_TARGET_DIR="C:\t"; pnpm tauri dev`. Conferir: roda com 1–5 pessoas do mock, brilho de quem fala, compartilhar → fileira no topo alinhada ao "CALL 1", Roda/tela pequena, trocar para Call 2 sem a janela encolher, arrastar pela barra "μChat", janela mínima 720×520 e maximizada.
- [ ] **Step 4: Commit** `test(e2e): nova tela da call`.
