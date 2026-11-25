<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { Backend, Participant, VideoSession, VideoStats, VideoStatus } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    backend,
    sharer,
    pequena,
    onroda,
    onampliar,
  }: {
    backend: Backend;
    sharer: Participant;
    pequena: boolean;
    onroda: () => void;
    onampliar?: () => void;
  } = $props();

  // Só o id: `sharer` é recriado a cada fala/mute e não pode religar o vídeo.
  const sharerId = $derived(sharer.id);

  let frame: HTMLElement | undefined = $state();
  let video: HTMLVideoElement | undefined = $state();
  let session = $state<VideoSession | null>(null);
  let status = $state<VideoStatus>('connecting');
  let message = $state('');
  let stats = $state<VideoStats | null>(null);
  let fullscreen = $state(false);

  // Travada, a última imagem continua visível com um aviso por cima.
  const hasFrame = $derived(status === 'playing' || status === 'stalled');
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
    const id = sharerId;
    if (!session || !video) return;
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
    // Pequena (roda): sem tela cheia por teclado/duplo-clique, só "Ampliar tela".
    if (pequena) return;
    if (e.key === 'f' || e.key === 'F') {
      e.preventDefault();
      void toggleFullscreen();
    }
  }
</script>

<svelte:document onfullscreenchange={() => (fullscreen = !!document.fullscreenElement && document.fullscreenElement === frame)} />

<section class="visor" aria-labelledby="visor-titulo">
  <h2 id="visor-titulo" class="sr-only">{sharer.isLocal ? 'Sua tela' : `Tela de ${sharer.name}`}</h2>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="moldura"
    class:cheia={fullscreen}
    class:pequena
    bind:this={frame}
    tabindex={pequena ? -1 : 0}
    role="region"
    aria-label={pequena ? 'Transmissão de tela' : 'Transmissão de tela. Tecle F para tela cheia.'}
    onkeydown={onKey}
    ondblclick={pequena ? undefined : toggleFullscreen}
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
    {:else if status === 'stalled'}
      <p class="espera aviso" role="status">{message}</p>
    {/if}

    {#if !pequena}
      <button type="button" class="botao roda" onclick={() => onroda()}>
        <Icon name="roda" size={16} />
        Roda
      </button>
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
    {:else}
      <!-- Pequena (roda): clicar na tela já amplia (ver Palco), mas teclado e
           leitor de tela precisam de um botão de verdade para a mesma ação. -->
      <button type="button" class="botao ampliar" onclick={() => onampliar?.()}>
        <Icon name="expand" size={16} />
        Ampliar tela
      </button>
    {/if}
  </div>
</section>

<style>
  .visor {
    display: grid;
    min-height: 0;
    height: 100%;
  }

  .moldura {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 0;
    height: 100%;
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

  .moldura.pequena .espera {
    font-size: 0.8rem;
  }

  .aviso {
    padding: 0.4rem 0.9rem;
    border-radius: var(--radius-pill);
    background: rgb(15 26 30 / 0.85);
  }

  .roda {
    position: absolute;
    top: 0.75rem;
    right: 0.75rem;
    background: rgb(15 26 30 / 0.85);
  }

  /* Cobre a moldura inteira: clique em qualquer ponto da tela pequena amplia
     (igual ao clique no div do Palco), e fica alcançável por teclado/leitor
     de tela como um botão de verdade, não só o clique do mouse. */
  .ampliar {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    gap: 0.2rem;
    border: none;
    background: transparent;
    color: #fff;
    font-size: 0.65rem;
    font-weight: 600;
    text-shadow: 0 1px 4px rgb(0 0 0 / 0.6);
    opacity: 0;
    transition: opacity 160ms ease;
  }

  .moldura:hover .ampliar,
  .moldura:focus-within .ampliar {
    opacity: 1;
    background: rgb(7 16 19 / 0.35);
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
