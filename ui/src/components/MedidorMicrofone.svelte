<script lang="ts">
  import { NIVEL_MAX_DB, NIVEL_MIN_DB, nivelParaFracao } from '../lib/micProcessing';

  /** Consulta o nível atual do microfone em dBFS (antes do corte do gate). */
  let {
    nivel,
    limiar,
    rotulo = 'Nível do microfone',
  }: { nivel: () => Promise<number>; limiar: number | null; rotulo?: string } = $props();

  const INTERVALO_MS = 66;
  let db = $state(NIVEL_MIN_DB);

  $effect(() => {
    let vivo = true;
    let consultando = false;
    const id = setInterval(async () => {
      if (consultando) return;
      consultando = true;
      try {
        const valor = await nivel();
        if (vivo) db = Number.isFinite(valor) ? valor : NIVEL_MIN_DB;
      } catch {
        // Sem áudio (fora da call ou microfone indisponível): o medidor só fica no mínimo.
        if (vivo) db = NIVEL_MIN_DB;
      } finally {
        consultando = false;
      }
    }, INTERVALO_MS);
    return () => {
      vivo = false;
      clearInterval(id);
    };
  });

  const valor = $derived(Math.round(Math.min(NIVEL_MAX_DB, Math.max(NIVEL_MIN_DB, db))));
  const cortado = $derived(limiar !== null && db < limiar);
</script>

<div
  class="medidor"
  role="meter"
  aria-label={rotulo}
  aria-valuemin={NIVEL_MIN_DB}
  aria-valuemax={NIVEL_MAX_DB}
  aria-valuenow={valor}
  aria-valuetext={cortado ? `${valor} dBFS, abaixo do limiar (cortado)` : `${valor} dBFS`}
>
  <div class="barra" class:cortado style:width="{nivelParaFracao(db) * 100}%"></div>
  {#if limiar !== null}
    <div class="limiar" data-limiar style:left="{nivelParaFracao(limiar) * 100}%"></div>
  {/if}
</div>

<style>
  .medidor {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: var(--color-trilho);
    overflow: hidden;
  }

  .barra {
    height: 100%;
    background: var(--color-sinal);
  }

  .barra.cortado {
    background: var(--color-valvula);
  }

  .limiar {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--color-giz);
  }
</style>
