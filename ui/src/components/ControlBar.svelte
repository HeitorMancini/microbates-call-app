<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
  import ShareSettings from './ShareSettings.svelte';

  let { store }: { store: RoomStore } = $props();

  let shareOpen = $state(false);
</script>

<div class="console" role="group" aria-label="Controles da chamada">
  <button
    type="button"
    class="controle"
    class:cortado={store.muted}
    aria-pressed={store.muted}
    aria-keyshortcuts="Control+Shift+M"
    title="Mudo (Ctrl+Shift+M)"
    onclick={() => store.toggleMute()}
  >
    <Icon name={store.muted ? 'mic-off' : 'mic'} size={28} />
    <span class="sr-only">Mudo</span>
  </button>
  <span class="sep" aria-hidden="true">/</span>
  <button
    type="button"
    class="controle"
    class:cortado={store.deafened}
    aria-pressed={store.deafened}
    aria-keyshortcuts="Control+Shift+D"
    title="Ensurdecer (Ctrl+Shift+D)"
    onclick={() => store.toggleDeafen()}
  >
    <Icon name={store.deafened ? 'headphones-off' : 'headphones'} size={28} />
    <span class="sr-only">Ensurdecer</span>
  </button>
  {#if store.backend.kind !== 'web'}
    <span class="sep" aria-hidden="true">/</span>
  {/if}
  {#if store.backend.kind === 'web'}
    <!-- Compartilhar tela não existe no navegador móvel. -->
  {:else if store.sharing}
    <button type="button" class="controle ativo" title="Parar de compartilhar" onclick={() => store.stopShare()}>
      <Icon name="screen-off" size={28} />
      <span class="sr-only">Parar de compartilhar</span>
    </button>
  {:else}
    <button
      type="button"
      class="controle"
      aria-haspopup="dialog"
      title="Compartilhar tela"
      onclick={() => (shareOpen = true)}
    >
      <Icon name="screen" size={28} />
      <span class="sr-only">Compartilhar tela</span>
    </button>
  {/if}
</div>

<Dialog bind:open={shareOpen} id="compartilhar" title="Compartilhar tela">
  <ShareSettings {store} onDone={() => (shareOpen = false)} />
</Dialog>

<style>
  .console {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 10px 14px;
    border-radius: var(--radius-pill);
    background: var(--color-placa);
    border: 1px solid var(--color-trilho);
  }

  .controle {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    padding: 0;
    border: 0;
    border-radius: 12px;
    background: none;
    color: var(--color-nevoa);
    transition:
      background-color 160ms ease,
      color 160ms ease;
  }

  .controle:hover {
    background: var(--color-valvula-fundo);
  }

  .controle.cortado {
    color: var(--color-corte);
  }

  .controle.ativo {
    background: var(--color-valvula);
    color: var(--color-valvula-texto);
  }

  .sep {
    color: var(--color-trilho);
    font-size: 26px;
    font-weight: 300;
  }
</style>
