<script lang="ts">
  import { blobatar } from 'blobatar';
  import { onDestroy } from 'svelte';
  import { corDoBlob, destaqueDe, tom } from '../lib/cor';
  import type { Participant } from '../lib/types';
  import type { Brilho } from '../lib/brilho';
  import Icon from './Icon.svelte';
  import Slider from './Slider.svelte';

  let {
    participant,
    pequeno,
    assistindo,
    onassistir,
    nivel,
    estilo,
    volume = 100,
    onvolume,
  }: {
    participant: Participant;
    pequeno: boolean;
    assistindo: boolean;
    onassistir?: () => void;
    nivel: () => number;
    estilo?: string;
    /** Volume local desta pessoa em % (0–200). */
    volume?: number;
    onvolume?: (percent: number) => void;
  } = $props();

  let menuAberto = $state(false);
  const temVolume = $derived(!participant.isLocal && !!onvolume);

  function abrirMenu(e: MouseEvent) {
    if (!temVolume) return;
    e.preventDefault();
    menuAberto = true;
  }

  function fecharFora(e: PointerEvent) {
    if (menuAberto && !(e.target as Element).closest('.volume-menu')) menuAberto = false;
  }

  function aoTeclar(e: KeyboardEvent) {
    if (menuAberto && e.key === 'Escape') menuAberto = false;
  }

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
  // Flag no nível do componente (não por execução do effect): cada evento de
  // participants chega como objeto novo e reexecuta o effect abaixo mesmo sem
  // destruir o Blob, então uma `vivo` presa no closure do effect cancelava o
  // import pendente nessas atualizações e o blob perdia o brilho para sempre.
  let destruido = false;

  // Só carrega o voice-glow (fora do pacote inicial) na primeira fala; sem
  // movimento reduzido não carrega nunca (contorno estático no lugar).
  $effect(() => {
    if (reduzMovimento || !participant.speaking || carregandoBrilho) return;
    carregandoBrilho = true;
    void import('../lib/brilho').then(({ montarBrilho }) => {
      if (destruido) return;
      brilho = montarBrilho(hostBrilho, { nivel });
    });
  });

  $effect(() => {
    brilho?.atualizar(participant.speaking, cores);
  });

  onDestroy(() => {
    destruido = true;
    brilho?.destruir();
  });
</script>

<svelte:window onpointerdown={fecharFora} onkeydown={aoTeclar} />

<li
  class="blob"
  class:pequeno
  class:falando={participant.speaking}
  data-testid="participante"
  data-speaking={participant.speaking}
  data-muted={participant.muted}
  data-pequeno={pequeno}
  style={estilo}
  oncontextmenu={abrirMenu}
>
  {#snippet avatarVisual()}
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
  {/snippet}
  {#if participant.sharingScreen}
    <!-- O botão envolve o blob inteiro (não só o selo) para o alvo de clique
         cobrir o avatar pequeno na roda; nada interativo fica aninhado dentro. -->
    <button
      type="button"
      class="aovivo"
      aria-label="AO VIVO — assistir a tela de {participant.name}"
      aria-pressed={assistindo}
      onclick={() => onassistir?.()}
    >
      <span class="selo-ao-vivo" aria-hidden="true">AO VIVO</span>
      {@render avatarVisual()}
    </button>
  {:else}
    {@render avatarVisual()}
  {/if}
  <span class="nome">
    {participant.name}{#if participant.isLocal}<span class="voce" aria-hidden="true">(você)</span>{/if}{#if description}<span
        class="sr-only">: {description}</span
      >{/if}
  </span>
  {#if menuAberto}
    <div class="volume-menu" role="group" aria-label="Volume de {participant.name}">
      <label for="volume-{participant.id}">Volume: {volume}%</label>
      <Slider
        id="volume-{participant.id}"
        value={volume}
        oninput={(percent) => onvolume?.(percent)}
      />
      <button type="button" class="botao" onclick={() => onvolume?.(100)}>Restaurar 100%</button>
    </div>
  {/if}
</li>

<style>
  .volume-menu {
    position: absolute;
    top: 100%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 20;
    display: grid;
    gap: 0.4rem;
    min-width: 11rem;
    padding: 0.6rem 0.75rem;
    border-radius: 0.6rem;
    background: var(--superficie, #1c1c22);
    border: 1px solid rgb(255 255 255 / 0.15);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.4);
    font-size: 0.8rem;
  }

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
    /* Troca palco/roda: mesma curva de 650 ms do transform do Palco. */
    transition:
      width 650ms cubic-bezier(0.2, 0.8, 0.2, 1),
      height 650ms cubic-bezier(0.2, 0.8, 0.2, 1);
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
    transition: font-size 650ms cubic-bezier(0.2, 0.8, 0.2, 1);
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
    /* Alvo de clique é o blob inteiro: o botão vira o próprio item do grid
       (mesma caixa do .avatar), não só o selo sobreposto. */
    position: relative;
    display: grid;
    place-items: center;
    border: none;
    padding: 0;
    background: none;
    cursor: pointer;
  }

  .selo-ao-vivo {
    position: absolute;
    top: -9px;
    left: 50%;
    z-index: 3;
    transform: translateX(-50%);
    padding: 2px 8px;
    border-radius: 999px;
    /* Mais escuro que o vermelho de destaque (#ff2e4d) para o texto branco em
       9px/7px bater 4.5:1 de contraste (WCAG AA). */
    background: #d81b45;
    color: #fff;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.04em;
    white-space: nowrap;
    pointer-events: none;
  }

  .pequeno .selo-ao-vivo {
    top: -6px;
    padding: 3px 4px;
    font-size: 7px;
  }
</style>
