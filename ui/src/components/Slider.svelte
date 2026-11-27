<script lang="ts">
  let {
    id,
    value,
    min = 0,
    max = 200,
    step = 5,
    oninput,
  }: {
    id: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    oninput?: (valor: number) => void;
  } = $props();

  const total = $derived(Math.round((max - min) / step) + 1);
  const ativo = $derived(Math.round((value - min) / step));
</script>

<!-- O <input type="range"> nativo fica por cima, invisível: teclado, leitor de tela e
     arrastar continuam nativos; os traços são só o desenho. -->
<div class="slider">
  <div class="tracos" aria-hidden="true">
    {#each { length: total } as _, i (i)}
      <span class="tr" class:atual={i === ativo} data-traco={i <= ativo}></span>
    {/each}
  </div>
  <input {id} type="range" {min} {max} {step} {value} oninput={(e) => oninput?.(e.currentTarget.valueAsNumber)} />
</div>

<style>
  .slider {
    position: relative;
    height: 28px;
  }

  .tracos {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    pointer-events: none;
  }

  .tr {
    width: 2px;
    height: 3px;
    border-radius: 2px;
    background: var(--color-valvula);
    transition:
      height 160ms var(--ease-mola),
      background-color 160ms;
  }

  .tr[data-traco='true'] {
    height: 16px;
    background: var(--color-sinal);
  }

  .tr.atual {
    height: 26px;
    background: var(--color-giz);
  }

  input {
    position: absolute;
    inset: 0;
    width: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .slider:has(input:focus-visible) {
    outline: 2px solid var(--color-sinal);
    outline-offset: 4px;
    border-radius: 6px;
  }

  @media (prefers-reduced-motion: reduce) {
    .tr {
      transition: none;
    }
  }
</style>
