<script lang="ts">
  import { avatarColor, initials } from '../lib/avatar';
  import type { Participant } from '../lib/types';
  import Icon from './Icon.svelte';

  let { participant, compact = false }: { participant: Participant; compact?: boolean } = $props();

  const color = $derived(avatarColor(participant.id + participant.name));
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
</script>

<li
  class="tile"
  class:compact
  class:falando={participant.speaking}
  data-testid="participante"
  data-speaking={participant.speaking}
  data-muted={participant.muted}
>
  <div class="avatar" style="--cor: {color}" aria-hidden="true">
    <span class="anel"></span>
    <span class="anel anel-2"></span>
    <span class="rosto">{initials(participant.name)}</span>
    {#if participant.muted}
      <span class="selo selo-mudo"><Icon name="mic-off" size={14} /></span>
    {/if}
    {#if participant.sharingScreen}
      <span class="selo selo-tela"><Icon name="screen" size={14} /></span>
    {/if}
  </div>
  <span class="nome">
    {participant.name}{#if participant.isLocal}<span class="voce" aria-hidden="true">(você)</span>{/if}{#if description}<span
        class="sr-only">: {description}</span
      >{/if}
  </span>
</li>

<style>
  .tile {
    --tam: 112px;
    display: grid;
    justify-items: center;
    gap: 0.75rem;
    min-width: 0;
    list-style: none;
  }

  .tile.compact {
    --tam: 56px;
    grid-template-columns: auto minmax(0, 1fr);
    justify-items: start;
    align-items: center;
    gap: 0.85rem;
  }

  .avatar {
    position: relative;
    width: var(--tam);
    height: var(--tam);
    display: grid;
    place-items: center;
  }

  .rosto {
    position: relative;
    z-index: 1;
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--cor);
    color: var(--color-breu);
    font-weight: 800;
    font-size: calc(var(--tam) * 0.34);
    font-variation-settings: 'wdth' 80;
    letter-spacing: -0.02em;
    transition: transform 220ms var(--ease-mola);
  }

  /* Anéis de "válvula" atrás do avatar: só transform/opacity, nada de layout. */
  .anel {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 3px solid var(--color-valvula);
    opacity: 0;
    transform: scale(0.92);
    transition:
      opacity 180ms ease,
      transform 220ms var(--ease-saida);
  }

  .falando .anel {
    opacity: 1;
    transform: scale(1.12);
    animation: pulso 1100ms var(--ease-saida) infinite;
  }

  .falando .anel-2 {
    animation-delay: 550ms;
  }

  .falando .rosto {
    transform: scale(1.04);
  }

  @keyframes pulso {
    0% {
      transform: scale(1.06);
      opacity: 0.95;
    }
    100% {
      transform: scale(1.38);
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .falando .anel {
      animation: none;
      transform: scale(1.1);
      opacity: 1;
    }

    .falando .anel-2 {
      opacity: 0;
    }
  }

  .selo {
    position: absolute;
    z-index: 2;
    bottom: 0;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 3px solid var(--color-breu);
  }

  .selo-mudo {
    right: 0;
    background: var(--color-corte);
    color: var(--color-breu);
  }

  .selo-tela {
    left: 0;
    background: var(--color-giz);
    color: var(--color-breu);
  }

  .compact .selo {
    width: 22px;
    height: 22px;
    border-width: 2px;
    bottom: -4px;
  }

  .compact .selo :global(svg) {
    width: 12px;
    height: 12px;
  }

  .nome {
    max-width: 100%;
    font-weight: 600;
    font-size: 1.05rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .voce {
    margin-left: 0.3em;
    font-weight: 400;
    color: var(--color-nevoa);
  }
</style>
