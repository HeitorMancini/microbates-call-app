<script lang="ts">
  import { prepararCena, semente, type Desenhar } from '../lib/cenas/motor';
  import { TAMANHO_SALA } from '../lib/janela';
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
    // A cena é gerada uma vez numa grade do tamanho da maior janela e a janela
    // só mostra o recorte central: ao crescer, revela mais cenário sem esticar.
    const cols = Math.ceil(Math.max(screen.width, TAMANHO_SALA.width) / CW);
    const rows = Math.ceil(Math.max(screen.height, TAMANHO_SALA.height) / CH);
    let visCols = 0;
    let visRows = 0;
    let desenhar: Desenhar | null = null;
    try {
      desenhar = prepararCena(cena, cols, rows, semente(seed));
    } catch (err) {
      console.error(`cenas.json, cena "${cena.nome}":`, err);
    }
    const glifos: string[] = new Array(cols * rows).fill('');
    const cores: string[] = new Array(cols * rows).fill('');
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
      const x0 = Math.floor((cols - visCols) / 2);
      const y0 = Math.floor((rows - visRows) / 2);
      ctx.clearRect(0, 0, visCols * CW, visRows * CH);
      for (let vy = 0; vy < visRows; vy++) {
        for (let vx = 0; vx < visCols; vx++) {
          const i = (y0 + vy) * cols + x0 + vx;
          const ch = glifos[i];
          if (!ch || ch === ' ') continue;
          ctx.fillStyle = cores[i];
          ctx.fillText(ch, vx * CW, vy * CH);
        }
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
      visCols = Math.min(cols, Math.ceil(width / CW));
      visRows = Math.min(rows, Math.ceil(height / CH));
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
