<script lang="ts">
  import { untrack } from 'svelte';
  import type { RoomStore } from '../lib/room.svelte';
  import { SCREEN_PRESETS, type ScreenPreset } from '../lib/types';

  let { store, onDone }: { store: RoomStore; onDone: () => void } = $props();

  let sourceId = $state<string | null>(null);
  let preset = $state<ScreenPreset>(untrack(() => store.sharePreset));
  let starting = $state(false);

  $effect(() => {
    void store.loadScreenSources().then(() => {
      sourceId ??= store.screenSources[0]?.id ?? null;
    });
  });

  async function start(e: SubmitEvent) {
    e.preventDefault();
    if (!sourceId) return;
    starting = true;
    const ok = await store.startShare(sourceId, preset);
    starting = false;
    if (ok) onDone();
  }
</script>

<form onsubmit={start}>
  <fieldset>
    <legend>O que mostrar</legend>
    {#if store.screenSources.length === 0}
      <p class="vazio">Procurando telas e janelas…</p>
    {/if}
    <div class="opcoes">
      {#each store.screenSources as source (source.id)}
        <label class="opcao">
          <input type="radio" name="fonte" value={source.id} bind:group={sourceId} />
          <span class="texto">
            <span class="titulo">{source.name}</span>
            <span class="sub">{source.kind === 'monitor' ? 'Monitor inteiro' : 'Janela'}</span>
          </span>
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset>
    <legend>Qualidade</legend>
    <div class="opcoes">
      {#each SCREEN_PRESETS as p (p.id)}
        <label class="opcao">
          <input type="radio" name="qualidade" value={p.id} bind:group={preset} />
          <span class="texto">
            <span class="titulo">{p.label}</span>
            <span class="sub">{p.hint}</span>
          </span>
        </label>
      {/each}
    </div>
  </fieldset>

  <div class="acoes">
    <button type="button" class="botao" onclick={onDone}>Cancelar</button>
    <button type="submit" class="botao botao-primario" disabled={!sourceId || starting} aria-busy={starting}>
      Compartilhar tela
    </button>
  </div>
</form>

<style>
  fieldset {
    margin: 0 0 1.25rem;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: 0.5rem;
    padding: 0;
    font-weight: 700;
  }

  .opcoes {
    display: grid;
    gap: 0.4rem;
  }

  .opcao {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-height: 52px;
    padding: 0.4rem 0.9rem;
    border-radius: var(--radius-botao);
    border: 1px solid var(--color-trilho);
    transition:
      border-color 160ms ease,
      background-color 160ms ease;
  }

  .opcao:hover {
    border-color: var(--color-nevoa);
  }

  .opcao:has(input:checked) {
    border-color: var(--color-valvula);
    background: var(--color-valvula-fundo);
  }

  input[type='radio'] {
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--color-valvula);
  }

  .texto {
    display: grid;
  }

  .titulo {
    font-weight: 600;
  }

  .sub,
  .vazio {
    margin: 0;
    font-size: 0.875rem;
    color: var(--color-nevoa);
  }

  .acoes {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
  }
</style>
