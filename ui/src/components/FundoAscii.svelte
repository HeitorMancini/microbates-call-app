<script lang="ts">
  import { prepararCena, semente, type Desenhar } from '../lib/cenas/motor';
  import { TAMANHO_SALA } from '../lib/janela';
  import { fundo } from '../lib/fundo.svelte';

  const CW = 6;
  const CH = 9;

  let { escuro = false }: { escuro?: boolean } = $props();
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
    // Última célula desenhada na tela (visível): só redesenha o que mudou.
    let desenhados: string[] = [];
    // Cada par glifo+cor vira um bitmap pequeno uma vez; o quadro só faz drawImage.
    // Cache não reativo, local ao efeito.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const atlas = new Map<string, HTMLCanvasElement>();
    let t = 0;

    function bitmap(ch: string, cor: string): HTMLCanvasElement {
      const chave = ch + cor;
      let b = atlas.get(chave);
      if (!b) {
        b = document.createElement('canvas');
        b.width = CW;
        b.height = CH;
        const c = b.getContext('2d')!;
        c.font = `${CH * 1.05}px ui-monospace, Consolas, monospace`;
        c.textBaseline = 'top';
        c.fillStyle = cor;
        c.fillText(ch, 0, 0);
        atlas.set(chave, b);
      }
      return b;
    }

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
      for (let vy = 0; vy < visRows; vy++) {
        for (let vx = 0; vx < visCols; vx++) {
          const i = (y0 + vy) * cols + x0 + vx;
          const ch = glifos[i];
          const chave = !ch || ch === ' ' ? '' : ch + cores[i];
          const k = vy * visCols + vx;
          if (desenhados[k] === chave) continue;
          desenhados[k] = chave;
          ctx.clearRect(vx * CW, vy * CH, CW, CH);
          if (chave) ctx.drawImage(bitmap(ch, cores[i]), vx * CW, vy * CH);
        }
      }
    }

    function medir() {
      const { width, height } = el.getBoundingClientRect();
      // Resolução 1:1 (sem escala por dpr): o fundo é decorativo e o custo de
      // pintar em tela cheia/maximizada era o que travava o app.
      el.width = Math.max(1, Math.round(width));
      el.height = Math.max(1, Math.round(height));
      visCols = Math.min(cols, Math.ceil(width / CW));
      visRows = Math.min(rows, Math.ceil(height / CH));
      desenhados = new Array(visCols * visRows).fill('\0');
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
        }, 125);

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
  <div class="veu" class:escuro></div>
</div>

<style>
  .fundo-ascii {
    position: fixed;
    inset: 0;
    z-index: 0;
    background: var(--color-breu);
  }

  .veu {
    position: absolute;
    inset: 0;
    /* Mesmo efeito do .fosco da tela de login. */
    opacity: 0;
    backdrop-filter: blur(1.5px);
    transition: opacity 380ms ease;
  }

  .veu.escuro {
    opacity: 1;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    background: #060608;
  }
</style>
