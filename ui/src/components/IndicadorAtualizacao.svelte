<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';

  let { store }: { store: RoomStore } = $props();

  let confirmando = $state(false);
  let instalando = $state(false);

  // Reiniciar derruba a call: o primeiro clique só pede confirmação.
  async function clicar() {
    if (!confirmando) {
      confirmando = true;
      return;
    }
    instalando = true;
    await store.installUpdate();
    instalando = false;
    confirmando = false;
  }
</script>

{#if store.update.status === 'downloading'}
  <span class="baixando" role="status">
    Baixando atualização{store.update.percent === null ? '…' : ` ${store.update.percent}%`}
  </span>
{:else if store.update.status === 'ready'}
  <button type="button" class="atualizar" aria-busy={instalando} onclick={clicar} onblur={() => (confirmando = false)}>
    {confirmando ? 'Isso encerra a call. Reiniciar?' : `Reiniciar para atualizar (v${store.update.version})`}
  </button>
{/if}

<style>
  .baixando {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-nevoa);
  }

  .atualizar {
    padding: 2px 10px;
    border: 1px solid var(--color-valvula);
    border-radius: 999px;
    background: var(--color-valvula-fundo);
    color: inherit;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
</style>
