<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { Backend, Participant, VideoSession, VideoStats, VideoStatus } from '../lib/types';
  import Icon from './Icon.svelte';

  let { backend, sharers }: { backend: Backend; sharers: Participant[] } = $props();

  let chosenId = $state<string | null>(null);
  const current = $derived(sharers.find((p) => p.id === chosenId) ?? sharers[0] ?? null);

  let frame: HTMLElement | undefined = $state();
  let video: HTMLVideoElement | undefined = $state();
  let session = $state<VideoSession | null>(null);
  let status = $state<VideoStatus>('connecting');
  let message = $state('');
  let stats = $state<VideoStats | null>(null);
  let fullscreen = $state(false);

  const hasFrame = $derived(status === 'playing');
  const resolution = $derived(stats ? `${stats.width}×${stats.height}` : '');
  const detail = $derived(stats ? `${stats.fps} fps · ${(stats.bitrateKbps / 1000).toFixed(1)} Mbps` : '');

  $effect(() => {
    let disposed = false;
    let opened: VideoSession | null = null;
    backend
      .openVideo()
      .then((s) => {
        if (disposed) s.close();
        else session = opened = s;
      })
      .catch((e: unknown) => {
        status = 'error';
        message = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Não foi possível abrir o vídeo.';
      });
    return () => {
      disposed = true;
      session = null;
      opened?.close();
    };
  });

  // Troca de transmissão: liga o <video> à tela escolhida.
  $effect(() => {
    const id = current?.id;
    if (!session || !video || !id) return;
    status = 'connecting';
    message = '';
    stats = null;
    return session.watch(id, video, (u) => {
      status = u.status;
      message = u.message ?? '';
      if (u.stats) stats = u.stats;
    });
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
    <video
      bind:this={video}
      class:visivel={hasFrame}
      data-testid="screen-video"
      aria-hidden="true"
      autoplay
      muted
      playsinline
    ></video>
    {#if !hasFrame}
      <p class="espera" role="status">
        {#if message}
          {message}
        {:else if status === 'connecting'}
          Conectando ao vídeo…
        {:else}
          Aguardando a imagem…
        {/if}
      </p>
    {/if}
    <div class="controles">
      {#if resolution && hasFrame}<span class="resolucao">{resolution}{detail ? ` · ${detail}` : ''}</span>{/if}
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

  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    opacity: 0;
    transition: opacity 240ms ease;
  }

  video.visivel {
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
