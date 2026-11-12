<script lang="ts">
  import JoinScreen from './components/JoinScreen.svelte';
  import RoomScreen from './components/RoomScreen.svelte';
  import type { RoomStore } from './lib/room.svelte';

  let { store }: { store: RoomStore } = $props();

  $effect(() => {
    void store.start();
    return () => store.stop();
  });
</script>

{#if store.backend.kind === 'mock'}
  <p class="modo-teste">Modo de demonstração: participantes e imagens simulados.</p>
{/if}

{#key store.inRoom}
  <div class="tela">
    {#if store.inRoom}
      <RoomScreen {store} />
    {:else}
      <JoinScreen {store} />
    {/if}
  </div>
{/key}

<style>
  .tela {
    height: 100%;
    animation: entrar 320ms var(--ease-saida);
  }

  @keyframes entrar {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }

  .modo-teste {
    position: fixed;
    top: 0.5rem;
    left: 50%;
    z-index: 10;
    margin: 0;
    padding: 0.2rem 0.75rem;
    transform: translateX(-50%);
    border-radius: var(--radius-pill);
    background: var(--color-placa);
    color: var(--color-nevoa);
    font-size: 0.8rem;
    pointer-events: none;
  }

  @media (max-width: 900px) {
    .modo-teste {
      display: none;
    }
  }
</style>
