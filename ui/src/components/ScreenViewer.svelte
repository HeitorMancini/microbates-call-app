<script lang="ts">
  import { onDestroy } from 'svelte';
  import { FrameRenderer } from '../lib/frames';
  import type { Backend, Participant, ScreenFrame, Unlisten } from '../lib/types';
  import Icon from './Icon.svelte';

  let { backend, sharers }: { backend: Backend; sharers: Participant[] } = $props();

  let chosenId = $state<string | null>(null);
  const current = $derived(sharers.find((p) => p.id === chosenId) ?? sharers[0] ?? null);

  let frame: HTMLElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let hasFrame = $state(false);
  let fullscreen = $state(false);
  let resolution = $state('');

  let renderer: FrameRenderer | null = null;

  $effect(() => {
    if (!canvas) return;
    const r = new FrameRenderer(canvas);
    r.onFirstFrame = () => (hasFrame = true);
    renderer = r;
    return () => r.dispose();
  });

  // Troca de transmissão: limpa o canvas até chegar o primeiro frame novo.
  $effect(() => {
    void current?.id;
    hasFrame = false;
    if (renderer) renderer.framesDrawn = 0;
  });

  $effect(() => {
    let unlisten: Unlisten | null = null;
    let disposed = false;
    void backend
      .onScreenFrame((f: ScreenFrame) => {
        if (f.participantId !== current?.id) return;
        const label = `${f.width}×${f.height}`;
        if (resolution !== label) resolution = label;
        renderer?.push(f);
      })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  onDestroy(() => {
    if (document.fullscreenElement && document.fullscreenElement === frame) void document.exitFullscreen();
  });

  async function toggleFullscreen() {
    if (!frame) return;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await frame.requestFullscreen();
    } catch {
      // Sem permissão de tela cheia: continua no modo normal.
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'f' || e.key === 'F') {
      e.preventDefault();
      void toggleFullscreen();
    }
  }
</script>

<svelte:document onfullscreenchange={() => (fullscreen = !!document.fullscreenElement && document.fullscreenElement === frame)} />

<section class="visor" aria-labelledby="visor-titulo">
  <header class="barra">
    <h2 id="visor-titulo">
      {#if current}
        {current.isLocal ? 'Sua tela' : `Tela de ${current.name}`}
      {/if}
    </h2>

    {#if sharers.length > 1}
      <div class="escolha" role="group" aria-label="Escolher transmissão">
        {#each sharers as s (s.id)}
          <button
            type="button"
            class="chip"
            aria-pressed={s.id === current?.id}
            onclick={() => (chosenId = s.id)}>{s.isLocal ? 'Você' : s.name}</button
          >
        {/each}
      </div>
    {/if}
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="moldura"
    class:cheia={fullscreen}
    bind:this={frame}
    tabindex="0"
    role="region"
    aria-label="Transmissão de tela. Tecle F para tela cheia."
    onkeydown={onKey}
    ondblclick={toggleFullscreen}
  >
    <canvas bind:this={canvas} class:visivel={hasFrame} data-testid="screen-canvas" aria-hidden="true"></canvas>
    {#if !hasFrame}
      <p class="espera" role="status">Aguardando a imagem…</p>
    {/if}
    <div class="controles">
      {#if resolution && hasFrame}<span class="resolucao">{resolution}</span>{/if}
      <button
        type="button"
        class="botao icone"
        onclick={toggleFullscreen}
        aria-label={fullscreen ? 'Sair da tela cheia' : 'Tela cheia'}
        title={fullscreen ? 'Sair da tela cheia (F)' : 'Tela cheia (F)'}
      >
        <Icon name={fullscreen ? 'shrink' : 'expand'} size={20} />
      </button>
    </div>
  </div>
</section>

<style>
  .visor {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 0.75rem;
    min-height: 0;
    height: 100%;
  }

  .barra {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  h2 {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
  }

  .escolha {
    display: flex;
    gap: 0.4rem;
  }

  .chip {
    min-height: 36px;
    padding: 0 0.9rem;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-trilho);
    background: transparent;
    font-weight: 600;
  }

  .chip[aria-pressed='true'] {
    background: var(--color-giz);
    border-color: var(--color-giz);
    color: var(--color-breu);
  }

  .moldura {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 0;
    border-radius: var(--radius-painel);
    background: #071013;
    overflow: hidden;
  }

  .moldura.cheia {
    border-radius: 0;
    background: #000;
  }

  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    opacity: 0;
    transition: opacity 240ms ease;
  }

  canvas.visivel {
    opacity: 1;
  }

  .espera {
    position: absolute;
    margin: 0;
    color: var(--color-nevoa);
  }

  .controles {
    position: absolute;
    right: 0.75rem;
    bottom: 0.75rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    opacity: 0.55;
    transition: opacity 160ms ease;
  }

  .moldura:hover .controles,
  .moldura:focus-within .controles {
    opacity: 1;
  }

  .resolucao {
    padding: 0.2rem 0.6rem;
    border-radius: var(--radius-pill);
    background: rgb(15 26 30 / 0.85);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }

  .icone {
    width: 44px;
    padding: 0;
    background: rgb(15 26 30 / 0.85);
  }
</style>
