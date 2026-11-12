<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import type { ConnectionStatus } from '../lib/types';
  import ControlBar from './ControlBar.svelte';
  import ParticipantList from './ParticipantList.svelte';
  import ScreenViewer from './ScreenViewer.svelte';

  let { store }: { store: RoomStore } = $props();

  const STATUS_LABEL: Record<ConnectionStatus, string> = {
    connected: 'Conectado',
    connecting: 'Conectando…',
    reconnecting: 'Reconectando…',
    error: 'Sem conexão',
    disconnected: 'Desconectado',
  };

  const speakingNow = $derived(store.participants.filter((p) => p.speaking).length);
  const headcount = $derived.by(() => {
    const n = store.participants.length;
    const people = `${n} ${n === 1 ? 'pessoa' : 'pessoas'}`;
    return speakingNow > 0 ? `${people}, ${speakingNow} falando` : people;
  });

  function onKey(e: KeyboardEvent) {
    if (!(e.ctrlKey && e.shiftKey)) return;
    const key = e.key.toLowerCase();
    if (key === 'm') {
      e.preventDefault();
      void store.toggleMute();
    } else if (key === 'd') {
      e.preventDefault();
      void store.toggleDeafen();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="sala">
  <header class="cabecalho">
    <div class="identidade">
      <h1 class="nome-sala">{store.roomName}</h1>
      <p class="servidor">{store.serverUrl}</p>
    </div>
    <div class="estado" data-status={store.status}>
      <span class="ponto" aria-hidden="true"></span>
      <span>{STATUS_LABEL[store.status]}</span>
      <span class="contagem">{headcount}</span>
    </div>
  </header>

  <main class="palco" class:com-tela={store.sharers.length > 0} aria-label="Sala {store.roomName}">
    {#if store.sharers.length > 0}
      <div class="area-tela">
        <ScreenViewer backend={store.backend} sharers={store.sharers} />
      </div>
      <aside class="trilho" aria-label="Pessoas">
        <ParticipantList participants={store.participants} compact />
      </aside>
    {:else}
      <div class="grade">
        <ParticipantList participants={store.participants} />
      </div>
    {/if}
  </main>

  {#if store.error}
    <div class="aviso" role="alert">
      <span>{store.error}</span>
      <button type="button" class="botao" onclick={() => store.dismissError()}>Entendi</button>
    </div>
  {/if}

  <footer class="rodape">
    <ControlBar {store} />
  </footer>

  <div class="sr-only" aria-live="polite" aria-atomic="true">{store.announcement}</div>
</div>

<style>
  .sala {
    height: 100%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto;
    gap: 1rem;
    padding: 1.25rem clamp(1rem, 3vw, 2rem) 1.25rem;
    box-sizing: border-box;
  }

  .cabecalho {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 0.75rem 2rem;
  }

  .identidade {
    min-width: 0;
  }

  .nome-sala {
    margin: 0;
    font-size: clamp(2rem, 5vw, 3.25rem);
    line-height: 0.95;
    font-weight: 800;
    font-variation-settings: 'wdth' 75;
    letter-spacing: -0.03em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .servidor {
    margin: 0.35rem 0 0;
    color: var(--color-nevoa);
    font-size: 0.9rem;
  }

  .estado {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.6rem;
    font-weight: 600;
  }

  .ponto {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-nevoa);
  }

  .estado[data-status='connected'] .ponto {
    background: var(--color-sinal);
  }

  .estado[data-status='reconnecting'] .ponto,
  .estado[data-status='connecting'] .ponto {
    background: var(--color-valvula);
    animation: piscar 900ms ease-in-out infinite alternate;
  }

  .estado[data-status='error'] .ponto {
    background: var(--color-corte);
  }

  @keyframes piscar {
    to {
      opacity: 0.25;
    }
  }

  .contagem {
    font-weight: 400;
    color: var(--color-nevoa);
  }

  .palco {
    min-height: 0;
    display: grid;
  }

  .grade {
    overflow-y: auto;
    align-self: center;
    padding: 1.5rem 0.5rem;
  }

  .palco.com-tela {
    grid-template-columns: minmax(0, 1fr) 240px;
    gap: 1.25rem;
  }

  .area-tela {
    min-height: 0;
  }

  .trilho {
    overflow-y: auto;
    padding: 0.5rem 0.25rem;
    min-height: 0;
  }

  .aviso {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0.6rem 0.6rem 1rem;
    border-radius: var(--radius-botao);
    background: var(--color-corte-fundo);
    border: 1px solid var(--color-corte);
  }

  @media (max-width: 760px) {
    .palco.com-tela {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(220px, 1fr) auto;
    }

    .trilho {
      max-height: 160px;
    }
  }
</style>
