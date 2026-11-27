<script lang="ts">
  import { fundo } from '../lib/fundo.svelte';
  import type { RoomStore } from '../lib/room.svelte';
  import type { DeviceKind } from '../lib/types';
  import Icon from './Icon.svelte';
  import { thresholdDbfs } from '../lib/micProcessing';
  import MedidorMicrofone from './MedidorMicrofone.svelte';
  import Slider from './Slider.svelte';

  let { store, onDone }: { store: RoomStore; onDone: () => void } = $props();

  const limiar = $derived(thresholdDbfs(store.micSensitivity));
  const rotuloSensibilidade = $derived(
    `Sensibilidade do microfone: ${store.micSensitivity >= 100 ? 'sem corte' : `${store.micSensitivity}%`}`,
  );

  $effect(() => {
    void store.loadDevices();
  });

  const web = $derived(store.backend.kind === 'web');
  let versao = $state('');
  let verificando = $state(false);
  let resultado = $state('');

  $effect(() => {
    store.backend
      .getAppVersion()
      .then((v) => (versao = v))
      .catch(() => {});
  });

  async function verificar() {
    verificando = true;
    resultado = '';
    try {
      const nova = await store.backend.checkForUpdate();
      resultado = nova ? 'Atualização encontrada!' : 'Você já está na versão mais recente.';
    } catch {
      resultado = 'Não foi possível verificar.';
    } finally {
      verificando = false;
    }
  }

  const groups: { kind: DeviceKind; label: string }[] = [
    { kind: 'input', label: 'Microfone' },
    { kind: 'output', label: 'Saída de áudio' },
  ];
</script>

<div class="campos">
  {#each groups as group (group.kind)}
    {@const list = group.kind === 'input' ? store.devices.inputs : store.devices.outputs}
    {#if !(web && group.kind === 'output' && list.length === 0)}
    <div class="grupo">
      <label for="dispositivo-{group.kind}">{group.label}</label>
      <select
        id="dispositivo-{group.kind}"
        class="campo"
        value={group.kind === 'input' ? store.selectedInput : store.selectedOutput}
        disabled={list.length === 0}
        onchange={(e) => store.selectDevice(group.kind, e.currentTarget.value)}
      >
        {#each list as device (device.id)}
          <option value={device.id}>{device.name}</option>
        {/each}
      </select>
    </div>
    {/if}
  {/each}
  {#if !web}
  <div class="grupo">
    <label for="volume-microfone">Volume do microfone: {store.micVolume}%</label>
    <Slider id="volume-microfone" value={store.micVolume} oninput={(percent) => store.setMicVolume(percent)} />
  </div>
  <div class="grupo">
    <label class="opcao" for="supressao-ruido">
      <input
        id="supressao-ruido"
        type="checkbox"
        checked={store.noiseSuppression}
        onchange={(e) => store.setNoiseSuppression(e.currentTarget.checked)}
      />
      Supressão de ruído
    </label>
    <p class="dica">Tira teclado, ventilador e outros barulhos de fundo da sua voz.</p>
  </div>
  <div class="grupo">
    <label for="sensibilidade-microfone">{rotuloSensibilidade}</label>
    <MedidorMicrofone nivel={() => store.backend.getMicLevel()} {limiar} />
    <Slider
      id="sensibilidade-microfone"
      value={store.micSensitivity}
      min={0}
      max={100}
      step={5}
      describedby="dica-sensibilidade"
      oninput={(percent) => store.setMicSensitivity(percent)}
    />
    <p class="dica" id="dica-sensibilidade">
      Quanto mais baixo, mais ruído de fundo é cortado. Fale e ajuste até a marca ficar logo abaixo do nível da sua voz.
    </p>
  </div>
  {/if}
  <div class="grupo">
    <span class="rotulo">Fundo</span>
    <div class="fundo-linha">
      <button type="button" class="botao" onclick={() => fundo.embaralhar()} disabled={!fundo.ligado}>
        <Icon name="shuffle" size={18} />
        Trocar fundo
      </button>
      <label class="opcao">
        <input
          type="checkbox"
          checked={fundo.ligado}
          onchange={(e) => fundo.alternar(e.currentTarget.checked)}
        />
        Fundo ASCII
      </label>
    </div>
  </div>
  <div class="grupo">
    <span class="rotulo">Versão {versao}</span>
    {#if !web}
      <div class="fundo-linha">
        <button type="button" class="botao" onclick={verificar} disabled={verificando}>
          {verificando ? 'Verificando…' : 'Verificar atualizações'}
        </button>
        <span role="status" class="opcao">{resultado}</span>
      </div>
    {/if}
  </div>
</div>
<div class="acoes">
  <button type="button" class="botao botao-primario" onclick={onDone}>Pronto</button>
</div>

<style>
  .campos {
    display: grid;
    gap: 1rem;
  }

  .grupo {
    display: grid;
    gap: 0.35rem;
  }

  label,
  .rotulo {
    font-weight: 600;
  }

  .fundo-linha {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .opcao {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 500;
  }

  .dica {
    margin: 0;
    font-size: 0.85rem;
    color: var(--color-nevoa);
  }

  select.campo {
    appearance: auto;
  }

  .acoes {
    display: flex;
    justify-content: flex-end;
    margin-top: 1.5rem;
  }
</style>
