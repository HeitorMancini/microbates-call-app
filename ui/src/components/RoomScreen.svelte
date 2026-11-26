<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import { arrastavel } from '../lib/janela';
  import type { ConnectionStatus } from '../lib/types';
  import ControlBar from './ControlBar.svelte';
  import Dialog from './Dialog.svelte';
  import DeviceSettings from './DeviceSettings.svelte';
  import Icon from './Icon.svelte';
  import IndicadorAtualizacao from './IndicadorAtualizacao.svelte';
  import Palco from './Palco.svelte';
  import SeletorSala from './SeletorSala.svelte';

  let { store }: { store: RoomStore } = $props();

  const STATUS_LABEL: Record<ConnectionStatus, string> = {
    connected: 'Conectado',
    connecting: 'Conectando…',
    reconnecting: 'Reconectando…',
    error: 'Sem conexão',
    disconnected: 'Desconectado',
  };

  let devicesOpen = $state(false);

  const volumeOf = (id: string) => store.volumeOf(id);
  const onvolume = (id: string, percent: number) => void store.setParticipantVolume(id, percent);

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
  <main class="area-palco" aria-label="Sala {store.roomName}">
    <Palco participants={store.participants} backend={store.backend} {volumeOf} {onvolume} />
  </main>

  <header class="barra" use:arrastavel={{ maximizarComDuploClique: true }}>
    <span class="marca"><b>μ</b>Chat</span>
    <IndicadorAtualizacao {store} />
    {#if store.status !== 'connected'}
      <span class="status">{STATUS_LABEL[store.status]}</span>
    {/if}
  </header>

  <div class="topo">
    <h1 class="nome-sala">
      <SeletorSala atual={store.roomName} ocupado={store.trocando} onescolher={(sala) => store.trocarSala(sala)} />
    </h1>
    <div class="acoes">
      <button type="button" class="quad" aria-label="Configurações" onclick={() => (devicesOpen = true)}>
        <Icon name="settings" size={26} />
      </button>
      <button type="button" class="quad sair" aria-label="Sair da call" onclick={() => store.leave()}>
        <Icon name="phone-leave" size={26} />
      </button>
    </div>
  </div>

  {#if store.error}
    <div class="aviso" role="alert">
      <span>{store.error}</span>
      <button type="button" class="botao" onclick={() => store.dismissError()}>Entendi</button>
    </div>
  {/if}

  <footer class="rodape">
    <ControlBar {store} />
    <button
      type="button"
      class="chat"
      aria-disabled="true"
      aria-label="Chat de texto (em breve)"
      title="Chat de texto · em breve"
    >
      <Icon name="chat" size={34} />
    </button>
  </footer>

  <div class="sr-only" aria-live="polite" aria-atomic="true">{store.announcement}</div>
</div>

<Dialog bind:open={devicesOpen} id="dispositivos" title="Dispositivos de áudio">
  <DeviceSettings {store} onDone={() => (devicesOpen = false)} />
</Dialog>

<style>
  .sala {
    position: relative;
    height: 100%;
  }

  .area-palco {
    position: absolute;
    inset: 0;
  }

  .barra {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 34px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0 12px 0 16px;
  }

  .marca {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.01em;
    color: var(--color-nevoa);
  }

  .marca b {
    color: #ff2e6e;
    font-weight: 800;
  }

  .status {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-nevoa);
  }

  .topo {
    position: absolute;
    top: 44px;
    left: 40px;
    right: 40px;
    z-index: 4;
    display: flex;
    align-items: center;
    justify-content: space-between;
    /* Em modo palco os blobs pequenos (y≈37–108) ficam sob esta faixa; só os
       controles de verdade (filhos diretos) devem capturar clique. */
    pointer-events: none;
  }

  .topo > :global(*) {
    pointer-events: auto;
  }

  .nome-sala {
    margin: 0;
    min-width: 0;
  }

  .acoes {
    display: flex;
    gap: 12px;
  }

  .quad {
    width: 56px;
    height: 56px;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: var(--color-placa);
    border: 1px solid var(--color-trilho);
    color: var(--color-nevoa);
    transition:
      background-color 160ms ease,
      transform 160ms ease,
      color 160ms ease;
  }

  .quad:hover {
    background: var(--color-valvula-fundo);
    transform: translateY(-1px);
  }

  .quad.sair:hover {
    color: var(--color-corte);
  }

  .aviso {
    position: absolute;
    top: 112px;
    left: 40px;
    right: 40px;
    z-index: 4;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0.6rem 0.6rem 1rem;
    border-radius: var(--radius-botao);
    background: var(--color-corte-fundo);
    border: 1px solid var(--color-corte);
  }

  .rodape {
    position: absolute;
    bottom: 36px;
    left: 40px;
    right: 40px;
    z-index: 4;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    /* Mesma lógica do .topo: não cobrir blobs/tela do palco por baixo. */
    pointer-events: none;
  }

  .rodape > :global(*) {
    pointer-events: auto;
  }

  .chat {
    position: relative;
    width: 76px;
    height: 76px;
    display: grid;
    place-items: center;
    border-radius: 18px;
    background: var(--color-placa);
    border: 1px solid var(--color-trilho);
    color: var(--color-nevoa);
    cursor: not-allowed;
  }

  .chat::after {
    content: '';
    position: absolute;
    top: 14px;
    right: 14px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ff2e6e;
    box-shadow: 0 0 0 3px var(--color-placa);
  }
</style>
