<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    open = $bindable(false),
    title,
    id,
    children,
  }: { open?: boolean; title: string; id: string; children: Snippet } = $props();

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  class="painel-dialogo"
  aria-labelledby="{id}-titulo"
  onclose={() => (open = false)}
  onclick={(e) => {
    // Clique no fundo (fora do conteúdo) fecha.
    if (e.target === dialog) open = false;
  }}
>
  <div class="topo">
    <h2 id="{id}-titulo">{title}</h2>
    <button type="button" class="fechar" aria-label="Fechar" onclick={() => (open = false)}>
      <Icon name="close" size={20} />
    </button>
  </div>
  {#if open}
    {@render children()}
  {/if}
</dialog>

<style>
  .topo {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1.25rem;
  }

  h2 {
    margin: 0;
    font-size: 1.35rem;
    font-weight: 700;
  }

  .fechar {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    margin-right: -0.5rem;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--color-nevoa);
  }

  .fechar:hover {
    color: var(--color-giz);
    background: var(--color-placa);
  }
</style>
