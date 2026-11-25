<script lang="ts">
  import { SALAS } from '../lib/types';
  import Icon from './Icon.svelte';

  let { atual, ocupado, onescolher }: { atual: string; ocupado: boolean; onescolher: (sala: string) => void } = $props();

  let aberto = $state(false);
  let raiz: HTMLDivElement | undefined = $state();
  let botao: HTMLButtonElement | undefined = $state();

  function abrir() {
    if (ocupado) return;
    aberto = true;
  }

  function fechar(devolverFoco: boolean) {
    aberto = false;
    if (devolverFoco) botao?.focus();
  }

  function escolher(sala: string) {
    onescolher(sala);
    fechar(true);
  }

  function aoClicarFora(e: MouseEvent) {
    if (aberto && raiz && !raiz.contains(e.target as Node)) fechar(false);
  }

  function aoTeclar(e: KeyboardEvent) {
    if (aberto && e.key === 'Escape') {
      e.preventDefault();
      fechar(true);
    }
  }
</script>

<svelte:window onclick={aoClicarFora} onkeydown={aoTeclar} />

<div class="seletor" class:aberto bind:this={raiz}>
  <button
    type="button"
    class="botao-sala"
    bind:this={botao}
    aria-disabled={ocupado}
    aria-haspopup="menu"
    aria-expanded={aberto}
    onclick={() => {
      if (ocupado) return;
      if (aberto) fechar(false);
      else abrir();
    }}
  >
    <span>{atual}</span>
    <Icon name="chevron-down" size={22} />
  </button>
  {#if aberto}
    <div class="menu" role="menu">
      {#each SALAS as sala (sala)}
        <button
          type="button"
          role="menuitemradio"
          aria-checked={sala === atual}
          class:atual={sala === atual}
          onclick={() => escolher(sala)}
        >
          {sala}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .seletor {
    position: relative;
  }

  .botao-sala {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px 6px 0;
    border: 0;
    background: none;
    color: var(--color-giz);
    font-size: 34px;
    font-weight: 800;
    letter-spacing: -0.01em;
    text-transform: uppercase;
  }

  .botao-sala[aria-disabled='true'] {
    opacity: 0.6;
    cursor: default;
  }

  .botao-sala :global(svg) {
    transition: transform 250ms ease;
  }

  .seletor.aberto .botao-sala :global(svg) {
    transform: rotate(180deg);
  }

  .menu {
    position: absolute;
    top: 52px;
    left: -6px;
    z-index: 6;
    min-width: 200px;
    padding: 6px;
    border-radius: 12px;
    background: var(--color-placa);
    border: 1px solid var(--color-trilho);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
    animation: menu-entra 180ms var(--ease-saida);
  }

  @keyframes menu-entra {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.98);
    }
  }

  .menu button {
    width: 100%;
    padding: 10px 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--color-nevoa);
    font-size: 15px;
    font-weight: 600;
    text-align: left;
  }

  .menu button:hover {
    background: var(--color-trilho);
    color: var(--color-giz);
  }

  .menu button.atual {
    color: var(--color-giz);
  }
</style>
