<script lang="ts">
  import { untrack } from 'svelte';
  import ControlesJanela from './components/ControlesJanela.svelte';
  import FundoAscii from './components/FundoAscii.svelte';
  import JoinScreen from './components/JoinScreen.svelte';
  import RoomScreen from './components/RoomScreen.svelte';
  import { animarPara, MINIMO_SALA, TAMANHO_ENTRADA, TAMANHO_SALA } from './lib/janela';
  import type { RoomStore } from './lib/room.svelte';

  let { store }: { store: RoomStore } = $props();

  type Fase = 'entrada' | 'abrindo' | 'sala' | 'fechando';
  const DISSOLVER_MS = 380;

  let fase = $state<Fase>('entrada');
  let rodada = 0;

  const esperar = (ms: number) => new Promise((r) => setTimeout(r, ms));

  $effect(() => {
    void store.start();
    return () => store.stop();
  });

  // Coreografia: entrar dissolve o card e expande a janela até a chamada;
  // sair escurece a chamada e encolhe a janela de volta ao card.
  $effect(() => {
    const dentro = store.presente;
    untrack(() => coreografar(dentro));
  });

  function coreografar(dentro: boolean) {
    const atual = ++rodada;
    void (async () => {
      if (dentro && (fase === 'entrada' || fase === 'fechando')) {
        fase = 'abrindo';
        await esperar(DISSOLVER_MS);
        if (rodada !== atual) return;
        await animarPara(TAMANHO_SALA, { minimo: MINIMO_SALA, redimensionavel: true });
        if (rodada === atual) fase = 'sala';
      } else if (!dentro && (fase === 'sala' || fase === 'abrindo')) {
        fase = 'fechando';
        await esperar(DISSOLVER_MS);
        if (rodada !== atual) return;
        await animarPara(TAMANHO_ENTRADA, { minimo: null, redimensionavel: false });
        if (rodada === atual) fase = 'entrada';
      }
    })();
  }

  const naSala = $derived(fase === 'sala' || fase === 'fechando');
</script>

{#if store.backend.kind === 'mock' && naSala}
  <p class="modo-teste">Modo de demonstração: participantes e imagens simulados.</p>
{/if}

<FundoAscii escuro={naSala || fase === 'abrindo'} />
<ControlesJanela maximizar={naSala} />

{#if naSala}
  <div class="tela sala" class:fechando={fase === 'fechando'}>
    <RoomScreen {store} />
  </div>
{:else}
  <div class="tela">
    <JoinScreen {store} saindo={fase === 'abrindo'} />
  </div>
{/if}

<style>
  .tela {
    position: relative;
    z-index: 1;
    height: 100%;
  }

  .sala {
    animation: abrir 420ms var(--ease-saida);
    transition: opacity 360ms ease;
  }

  .fechando {
    opacity: 0;
  }

  @keyframes abrir {
    from {
      opacity: 0;
      transform: scale(0.96);
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
