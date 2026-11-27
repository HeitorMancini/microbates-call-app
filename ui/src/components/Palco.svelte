<script lang="ts">
  import { untrack } from 'svelte';
  import { TAMANHO_SALA } from '../lib/janela';
  import { ordemDeChegada, posicoes, type Modo } from '../lib/roda';
  import { criarEnvelope } from '../lib/voz';
  import type { Backend, Participant } from '../lib/types';
  import Blob from './Blob.svelte';
  import ScreenViewer from './ScreenViewer.svelte';
  import Slider from './Slider.svelte';

  let {
    participants,
    backend,
    volumeOf,
    onvolume,
  }: {
    participants: Participant[];
    backend: Backend;
    volumeOf?: (id: string) => number;
    onvolume?: (id: string, percent: number) => void;
  } = $props();

  // Metade do corpo do avatar (96px/46px no Blob), para centrar no ponto da roda.
  const METADE_GRANDE = 48;
  const METADE_PEQUENA = 23;

  let raiz: HTMLDivElement | undefined = $state();
  let largura = $state(TAMANHO_SALA.width);
  let altura = $state(TAMANHO_SALA.height);

  $effect(() => {
    if (!raiz) return;
    const ro = new ResizeObserver((entradas) => {
      const retangulo = entradas[0]?.contentRect;
      if (!retangulo) return;
      largura = retangulo.width;
      altura = retangulo.height;
    });
    ro.observe(raiz);
    return () => ro.disconnect();
  });

  // Ordem estável: quem sai some, quem entra vai para o fim; o local vem primeiro.
  // `anteriorOrdem` é um detalhe de implementação não reativo: só guarda o
  // resultado anterior para alimentar a próxima chamada de `ordemDeChegada`.
  let anteriorOrdem: string[] = [];
  const ordem = $derived.by(() => {
    anteriorOrdem = ordemDeChegada(anteriorOrdem, participants);
    return anteriorOrdem;
  });

  const compartilhando = $derived(participants.filter((p) => p.sharingScreen));
  const temTela = $derived(compartilhando.length > 0);

  let modo = $state<Modo>('roda');
  let assistindo = $state<string | null>(null);
  let anteriorCompartilhando = 0;

  // Palco entra quando alguém começa a compartilhar (0 → ≥1) e sai quando o
  // último para (≥1 → 0); quem se assiste cai para o próximo que ainda compartilha.
  $effect(() => {
    const lista = compartilhando;
    if (anteriorCompartilhando === 0 && lista.length > 0) modo = 'palco';
    else if (anteriorCompartilhando > 0 && lista.length === 0) modo = 'roda';
    anteriorCompartilhando = lista.length;

    const atual = untrack(() => assistindo);
    if (lista.length === 0) {
      if (atual !== null) assistindo = null;
    } else if (!atual || !lista.some((p) => p.id === atual)) {
      assistindo = lista[0].id;
    }
  });

  const pequena = $derived(modo === 'roda');
  const layout = $derived(posicoes(ordem, { modo, temTela, largura, altura }));
  // Cai para o próximo que ainda compartilha em vez de passar por `null`
  // enquanto o efeito acima ainda não escolheu `assistindo`: sem isso o
  // ScreenViewer desmontava e remontava por um instante (vídeo reabria) toda
  // vez que a pessoa assistida parava de compartilhar.
  const sharer = $derived(compartilhando.find((p) => p.id === assistindo) ?? compartilhando[0] ?? null);

  // Um envelope de voz compartilhado por todos os blobs do palco.
  const envelope = criarEnvelope();
  const nivel = () => envelope(performance.now());

  function estiloDoBlob(id: string): string {
    if (id === ajustando) {
      // Centro da janela, um pouco acima para sobrar espaço ao controle embaixo.
      return `z-index: 41; transform: translate(calc(${largura / 2}px - 50%), ${altura * 0.38 - METADE_GRANDE}px)`;
    }
    const ponto = layout.blobs.get(id);
    if (!ponto) return 'display: none';
    const metade = ponto.pequeno ? METADE_PEQUENA : METADE_GRANDE;
    // translateX usa calc(... - 50%) para centrar o item no ponto não importa a
    // largura real do <li> (varia com o tamanho do nome); translateY soma a
    // metade do avatar porque ele é a primeira linha do grid, alinhada ao topo.
    return `transform: translate(calc(${ponto.x}px - 50%), ${ponto.y - metade}px)`;
  }

  function aoClicarNaTela(e: MouseEvent) {
    if (pequena && !(e.target as Element).closest('button')) modo = 'palco';
  }

  function voltarParaRoda() {
    modo = 'roda';
  }

  function voltarParaPalco() {
    modo = 'palco';
  }

  // Ajuste de volume: o blob escolhido vai para o centro, o resto escurece.
  let ajustando = $state<string | null>(null);
  const alvoAjuste = $derived(participants.find((p) => p.id === ajustando) ?? null);

  function abrirAjuste(id: string) {
    ajustando = id;
  }

  function fecharAjuste() {
    ajustando = null;
  }

  function aoTeclar(e: KeyboardEvent) {
    if (ajustando && e.key === 'Escape') fecharAjuste();
  }
</script>

<svelte:window onkeydown={aoTeclar} />

<div class="palco" bind:this={raiz}>
  {#if participants.length === 0}
    <p class="vazio">Ninguém por aqui ainda. Chame a turma para a sala.</p>
  {:else}
    <ul class="pessoas" aria-label="Pessoas na sala ({participants.length})">
      {#each ordem as id (id)}
        {@const participant = participants.find((p) => p.id === id)}
        {#if participant}
          <Blob
            {participant}
            pequeno={id === ajustando ? false : (layout.blobs.get(id)?.pequeno ?? false)}
            assistindo={id === assistindo}
            onassistir={() => (assistindo = id)}
            {nivel}
            oncontexto={onvolume ? () => abrirAjuste(id) : undefined}
            estilo={estiloDoBlob(id)}
          />
        {/if}
      {/each}
    </ul>
  {/if}

  {#if alvoAjuste}
    <!-- Fixo e acima de tudo (barra, controles): clicar na área escurecida sai do ajuste. -->
    <div class="escuro" role="presentation" onpointerdown={fecharAjuste}></div>
    <div class="ajuste" role="group" aria-label="Volume de {alvoAjuste.name}">
      <label for="volume-{alvoAjuste.id}">Volume de {alvoAjuste.name}: {volumeOf?.(alvoAjuste.id) ?? 100}%</label>
      <Slider
        id="volume-{alvoAjuste.id}"
        grande
        value={volumeOf?.(alvoAjuste.id) ?? 100}
        oninput={(percent) => onvolume?.(alvoAjuste.id, percent)}
      />
      <button type="button" class="botao" onclick={() => onvolume?.(alvoAjuste.id, 100)}>Restaurar 100%</button>
    </div>
  {/if}

  {#if temTela && sharer && layout.tela}
    <div
      class="tela"
      class:pequena
      role="presentation"
      data-testid="palco-tela"
      style="left: {layout.tela.x}px; top: {layout.tela.y}px; width: {layout.tela.w}px; height: {layout.tela.h}px"
      onclick={aoClicarNaTela}
    >
      <ScreenViewer {backend} {sharer} {pequena} onroda={voltarParaRoda} onampliar={voltarParaPalco} />
    </div>
  {/if}
</div>

<style>
  .palco {
    position: absolute;
    inset: 0;
    /* Áreas vazias não capturam clique: a barra, o topo e o rodapé ficam por cima. */
    pointer-events: none;
  }

  .vazio {
    position: absolute;
    top: 50%;
    left: 50%;
    width: min(100%, 420px);
    margin: 0;
    transform: translate(-50%, -50%);
    color: var(--color-nevoa);
    text-align: center;
  }

  .pessoas {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Alcança o <li> que o Blob renderiza na raiz (sem wrapper, ver controller ruling).
     Especificidade (li + .pessoas + .blob) de propósito, para vencer o
     `position: relative` do próprio Blob independente da ordem de bundling. */
  .pessoas :global(li.blob) {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: auto;
    transition: transform 650ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .escuro {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(3 3 5 / 0.8);
    pointer-events: auto;
    animation: aparecer 250ms ease;
  }

  .ajuste {
    position: fixed;
    left: 50%;
    bottom: 72px;
    z-index: 41;
    transform: translateX(-50%);
    width: min(440px, calc(100% - 80px));
    display: grid;
    gap: 14px;
    justify-items: center;
    pointer-events: auto;
  }

  @keyframes aparecer {
    from {
      opacity: 0;
    }
  }

  .ajuste label {
    font-weight: 600;
    font-size: 17px;
  }

  .ajuste :global(.slider) {
    width: 100%;
  }

  .tela {
    position: absolute;
    pointer-events: auto;
    border-radius: var(--radius-painel);
    overflow: hidden;
    background: var(--color-casco);
    transition:
      left 650ms cubic-bezier(0.2, 0.8, 0.2, 1),
      top 650ms cubic-bezier(0.2, 0.8, 0.2, 1),
      width 650ms cubic-bezier(0.2, 0.8, 0.2, 1),
      height 650ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
</style>
