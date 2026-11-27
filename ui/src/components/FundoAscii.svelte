<script lang="ts">
  import { prepararCena, semente, type Desenhar } from '../lib/cenas/motor';
  import { fundo } from '../lib/fundo.svelte';

  const CW = 6;
  const CH = 9;

  let canvas = $state<HTMLCanvasElement>();

  $effect(() => {
    const abertura = fundo.ligado ? fundo.abertura : null;
    if (!canvas || !abertura) return;
    const { cena, seed } = abertura;
    const el = canvas;
    const ctx = el.getContext('2d');
    if (!ctx) return;
    const quieto = matchMedia('(prefers-reduced-motion: reduce)').matches;
    let cols = 0;
    let rows = 0;
    let desenhar: Desenhar | null = null;
    let glifos: string[] = [];
    let cores: string[] = [];
    let t = 0;

    function quadro() {
      if (!desenhar || !ctx) return;
      glifos.fill('');
      desenhar((x, y, ch, cor) => {
        if (x < 0 || y < 0 || x >= cols || y >= rows) return;
        const i = y * cols + x;
        glifos[i] = ch;
        cores[i] = cor;
      }, t);
      ctx.clearRect(0, 0, cols * CW, rows * CH);
      for (let i = 0; i < glifos.length; i++) {
        const ch = glifos[i];
        if (!ch || ch === ' ') continue;
        ctx.fillStyle = cores[i];
        ctx.fillText(ch, (i % cols) * CW, Math.floor(i / cols) * CH);
      }
    }

    function medir() {
      const { width, height } = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      el.width = Math.max(1, Math.round(width * dpr));
      el.height = Math.max(1, Math.round(height * dpr));
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx!.font = `${CH * 1.05}px ui-monospace, Consolas, monospace`;
      ctx!.textBaseline = 'top';
      cols = Math.ceil(width / CW);
      rows = Math.ceil(height / CH);
      glifos = new Array(cols * rows).fill('');
      cores = new Array(cols * rows).fill('');
      try {
        desenhar =         prepararCena(cena, cols, rows, semente(seed));
      } catch (err) {
                console.error(`cenas.json, cena "${cena.nome}":`, err);
        desenhar = null;
      }
      quadro();
    }

    const ro = new ResizeObserver(medir);
    ro.observe(el);
    medir();

    const timer = quieto
      ? undefined
      : setInterval(() => {
          if (document.hidden) return;
          t++;
          quadro();
        }, 100);

    return () => {
      ro.disconnect();
      clearInterval(timer);
    };
  });
</script>

<div class="fundo-ascii" aria-hidden="true">
  {#if fundo.ligado && fundo.abertura}
    <canvas bind:this={canvas}></canvas>
  {/if}
</div>

<style>
  .fundo-ascii {
    position: fixed;
    inset: 0;
    z-index: 0;
    background: var(--color-breu);
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    background: #060608;
  }
</style>
