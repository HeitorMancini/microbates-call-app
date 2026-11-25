<script lang="ts">
  import { alternarMaximizar, fechar, minimizar } from '../lib/janela';

  let { maximizar = true }: { maximizar?: boolean } = $props();
</script>

<div class="controles">
  <button type="button" aria-label="Minimizar" onclick={minimizar}>
    <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 5h6" /></svg>
  </button>
  {#if maximizar}
    <button type="button" aria-label="Maximizar" onclick={alternarMaximizar}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><rect x="2" y="2" width="6" height="6" rx="1" /></svg>
    </button>
  {/if}
  <button type="button" class="fechar" aria-label="Fechar" onclick={fechar}>
    <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5l5 5M7.5 2.5l-5 5" /></svg>
  </button>
</div>

<style>
  .controles {
    position: fixed;
    top: 10px;
    right: 12px;
    z-index: 20;
    display: flex;
    gap: 5px;
  }

  button {
    width: 22px;
    height: 22px;
    padding: 0;
    display: grid;
    place-items: center;
    border: none;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.07);
    color: #a5a5ad;
    transition:
      background-color 140ms ease,
      color 140ms ease;
  }

  button:hover {
    background: rgb(255 255 255 / 0.14);
    color: var(--color-giz);
  }

  .fechar:hover {
    background: var(--color-valvula);
    color: var(--color-valvula-texto);
  }

  svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
  }
</style>
