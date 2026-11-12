<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import type { DeviceKind } from '../lib/types';

  let { store, onDone }: { store: RoomStore; onDone: () => void } = $props();

  $effect(() => {
    void store.loadDevices();
  });

  const groups: { kind: DeviceKind; label: string }[] = [
    { kind: 'input', label: 'Microfone' },
    { kind: 'output', label: 'Saída de áudio' },
  ];
</script>

<div class="campos">
  {#each groups as group (group.kind)}
    {@const list = group.kind === 'input' ? store.devices.inputs : store.devices.outputs}
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
  {/each}
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

  label {
    font-weight: 600;
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
