<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import Dialog from './Dialog.svelte';
  import DeviceSettings from './DeviceSettings.svelte';
  import Icon from './Icon.svelte';
  import ShareSettings from './ShareSettings.svelte';

  let { store }: { store: RoomStore } = $props();

  let devicesOpen = $state(false);
  let shareOpen = $state(false);
</script>

<div class="console" role="group" aria-label="Controles da chamada">
  <div class="grupo">
    <button
      type="button"
      class="controle"
      class:cortado={store.muted}
      aria-pressed={store.muted}
      aria-keyshortcuts="Control+Shift+M"
      title="Mudo (Ctrl+Shift+M)"
      onclick={() => store.toggleMute()}
    >
      <Icon name={store.muted ? 'mic-off' : 'mic'} />
      <span>Mudo</span>
    </button>
    <button
      type="button"
      class="controle"
      class:cortado={store.deafened}
      aria-pressed={store.deafened}
      aria-keyshortcuts="Control+Shift+D"
      title="Ensurdecer (Ctrl+Shift+D)"
      onclick={() => store.toggleDeafen()}
    >
      <Icon name={store.deafened ? 'headphones-off' : 'headphones'} />
      <span>Ensurdecer</span>
    </button>
    <button
      type="button"
      class="controle"
      aria-haspopup="dialog"
      onclick={() => (devicesOpen = true)}
    >
      <Icon name="sliders" />
      <span>Dispositivos</span>
    </button>
  </div>

  <div class="grupo">
    {#if store.sharing}
      <button type="button" class="controle ativo" onclick={() => store.stopShare()}>
        <Icon name="screen-off" />
        <span>Parar de compartilhar</span>
      </button>
    {:else}
      <button type="button" class="controle" aria-haspopup="dialog" onclick={() => (shareOpen = true)}>
        <Icon name="screen" />
        <span>Compartilhar tela</span>
      </button>
    {/if}
    <button type="button" class="controle sair" onclick={() => store.leave()}>
      <Icon name="leave" />
      <span>Sair</span>
    </button>
  </div>
</div>

<Dialog bind:open={devicesOpen} id="dispositivos" title="Dispositivos de áudio">
  <DeviceSettings {store} onDone={() => (devicesOpen = false)} />
</Dialog>

<Dialog bind:open={shareOpen} id="compartilhar" title="Compartilhar tela">
  <ShareSettings {store} onDone={() => (shareOpen = false)} />
</Dialog>

<style>
  .console {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.75rem;
    border-radius: var(--radius-painel);
    background: var(--color-casco);
    border: 1px solid var(--color-placa);
  }

  .grupo {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .controle {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    min-height: 48px;
    padding: 0 1rem;
    border-radius: var(--radius-botao);
    border: 1px solid transparent;
    background: var(--color-placa);
    font-weight: 600;
    transition:
      transform 140ms var(--ease-saida),
      background-color 160ms ease,
      color 160ms ease,
      border-color 160ms ease;
  }

  .controle:hover {
    border-color: var(--color-nevoa);
  }

  .controle:active {
    transform: scale(0.95);
  }

  .controle.cortado {
    background: var(--color-corte-fundo);
    border-color: var(--color-corte);
    color: var(--color-corte);
  }

  .controle.ativo {
    background: var(--color-valvula);
    color: var(--color-breu);
  }

  .controle.sair {
    background: transparent;
    border-color: var(--color-corte);
    color: var(--color-corte);
  }

  .controle.sair:hover {
    background: var(--color-corte-fundo);
  }

  @media (max-width: 900px) {
    .controle span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
    }

    .controle {
      width: 48px;
      padding: 0;
      justify-content: center;
    }
  }
</style>
