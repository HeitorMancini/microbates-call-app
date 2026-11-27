<script lang="ts">
  let {
    id,
    value,
    min = 0,
    max = 200,
    step = 1,
    grande = false,
    oninput,
  }: {
    id: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    grande?: boolean;
    oninput?: (valor: number) => void;
  } = $props();

  const TOTAL = 41;
  // Quanto de cada traço está "cheio" (0–1): os anteriores ao valor inteiros, o
  // da ponta parcial, os demais zero.
  const posicao = $derived(((value - min) / (max - min)) * TOTAL);
  // Traço em andamento (branco) até ficar cheio; aí vira verde e o próximo assume.
  const ponta = $derived(Math.floor(posicao));
  const cheio = (i: number) => Math.min(1, Math.max(0, posicao - i));
</script>

<!-- O <input type="range"> nativo fica por cima, invisível: teclado, leitor de tela e
     arrastar continuam nativos; os traços são só o desenho. -->
<div class="slider" class:grande>
  <div class="tracos" aria-hidden="true">
    {#each { length: TOTAL } as _, i (i)}
      <span class="tr" class:atual={i === ponta} style:--f={cheio(i)} data-traco={cheio(i) > 0}></span>
    {/each}
  </div>
  <input {id} type="range" {min} {max} {step} {value} oninput={(e) => oninput?.(e.currentTarget.valueAsNumber)} />
</div>

<style>
  .slider {
    --alt-ponto: 3px;
    --alt-traco: 16px;
    position: relative;
    height: 28px;
  }

  .slider.grande {
    --alt-ponto: 4px;
    --alt-traco: 30px;
    height: 44px;
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
    --f: 0;
    width: 3px;
    height: calc(var(--alt-ponto) + (var(--alt-traco) - var(--alt-ponto)) * var(--f));
    border-radius: 2px;
    background: var(--color-valvula);
    transition:
      height 160ms var(--ease-mola),
      background-color 160ms;
  }

  .tr[data-traco='true'] {
    background: var(--color-sinal);
  }

  .tr.atual {
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
