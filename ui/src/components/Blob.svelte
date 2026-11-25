<script lang="ts">
  import { blobatar } from 'blobatar';
  import { onDestroy } from 'svelte';
  import { corDoBlob, destaqueDe, tom } from '../lib/cor';
  import type { Participant } from '../lib/types';
  import type { Brilho } from '../lib/brilho';
  import Icon from './Icon.svelte';

  let {
    participant,
    pequeno,
    assistindo,
    onassistir,
    nivel,
    estilo,
  }: {
    participant: Participant;
    pequeno: boolean;
    assistindo: boolean;
    onassistir?: () => void;
    nivel: () => number;
    estilo?: string;
  } = $props();

  const svg = $derived(blobatar(participant.name));
  const destaque = $derived(destaqueDe(corDoBlob(svg)));
  const cores = $derived([tom(destaque, 0.08, 0.9), destaque.cor, tom(destaque, -0.08, 1), tom(destaque, 0.15, 0.6)]);
  const description = $derived(
    [
      participant.isLocal ? 'você' : '',
      participant.speaking ? 'falando' : '',
      participant.muted ? 'microfone desligado' : '',
      participant.sharingScreen ? 'compartilhando tela' : '',
    ]
      .filter(Boolean)
      .join(', '),
  );

  const reduzMovimento = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  let hostBrilho: HTMLSpanElement;
  let brilho = $state<Brilho | null>(null);
  let carregandoBrilho = false;

  // Só carrega o voice-glow (fora do pacote inicial) na primeira fala; sem
  // movimento reduzido não carrega nunca (contorno estático no lugar).
  $effect(() => {
    if (reduzMovimento || !participant.speaking || carregandoBrilho) return;
    carregandoBrilho = true;
    let vivo = true;
    void import('../lib/brilho').then(({ montarBrilho }) => {
      if (!vivo) return;
      brilho = montarBrilho(hostBrilho, { nivel });
    });
    return () => {
      vivo = false;
    };
  });

  $effect(() => {
    brilho?.atualizar(participant.speaking, cores);
  });

  onDestroy(() => brilho?.destruir());
</script>

<li
  class="blob"
  class:pequeno
  class:falando={participant.speaking}
  data-testid="participante"
  data-speaking={participant.speaking}
  data-muted={participant.muted}
  data-pequeno={pequeno}
  style={estilo}
>
  {#if participant.sharingScreen}
    <button
      type="button"
      class="aovivo"
      aria-label="Assistir a tela de {participant.name}"
      aria-pressed={assistindo}
      onclick={() => onassistir?.()}
    >
      AO VIVO
    </button>
  {/if}
  <div class="avatar" aria-hidden="true">
    <span
      class="brilho"
      class:estatico={reduzMovimento}
      style:--cor-contorno={destaque.cor}
      bind:this={hostBrilho}
    ></span>
    <img class="imagem" alt="" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} />
    {#if participant.muted}
      <span class="selo selo-mudo"><Icon name="mic-off" size={pequeno ? 10 : 14} /></span>
    {/if}
  </div>
  <span class="nome">
    {participant.name}{#if participant.isLocal}<span class="voce" aria-hidden="true">(você)</span>{/if}{#if description}<span
        class="sr-only">: {description}</span
      >{/if}
  </span>
</li>

<style>
  .blob {
    position: relative;
    display: grid;
    justify-items: center;
    gap: 20px;
    list-style: none;
  }

  .blob.pequeno {
    gap: 10px;
  }

  .avatar {
    position: relative;
    width: 96px;
    height: 96px;
    display: grid;
    place-items: center;
  }

  .pequeno .avatar {
    width: 46px;
    height: 46px;
  }

  .imagem {
    position: relative;
    z-index: 1;
    width: 130%;
    height: 130%;
    border-radius: 50%;
  }

  .brilho {
    position: absolute;
    z-index: 0;
    left: -22%;
    right: -22%;
    top: -4%;
    bottom: -16%;
    border-radius: 50%;
    pointer-events: none;
  }

  .falando .brilho.estatico {
    outline: 3px solid var(--cor-contorno, #fff);
    outline-offset: 2px;
  }

  .selo {
    position: absolute;
    z-index: 2;
    right: 0;
    bottom: 0;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 3px solid var(--color-breu);
    background: var(--color-corte);
    color: var(--color-breu);
  }

  .pequeno .selo {
    width: 16px;
    height: 16px;
    border-width: 2px;
  }

  .nome {
    max-width: 128px;
    font-weight: 600;
    font-size: 17px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pequeno .nome {
    max-width: 72px;
    font-size: 12px;
  }

  .voce {
    margin-left: 0.3em;
    font-weight: 400;
    color: #fff;
  }

  .aovivo {
    position: absolute;
    top: -9px;
    left: 50%;
    z-index: 3;
    transform: translateX(-50%);
    padding: 2px 8px;
    border: none;
    border-radius: 999px;
    background: #ff2e4d;
    color: #fff;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.04em;
    cursor: pointer;
  }
</style>
