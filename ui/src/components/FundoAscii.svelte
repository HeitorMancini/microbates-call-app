<script lang="ts">
  import dados from '../lib/cenas/cenas.json';
  import { prepararCena, semente, sortearCena, validarCenas, type Desenhar } from '../lib/cenas/motor';

  const CHAVE = 'sheeshcord:ultima-cena';
  const CW = 6;
  const CH = 9;

  let { pausado = false }: { pausado?: boolean } = $props();

  let canvas: HTMLCanvasElement;

  function lerAnterior(): string | null {
    try {
      return localStorage.getItem(CHAVE);
    } catch {
      return null;
    }
  }

  // Uma "abertura": sorteia a cena e a semente uma vez por montagem do componente.
  const abertura = (() => {
    try {
      const cena = sortearCena(validarCenas(dados), lerAnterior());
      try {
        localStorage.setItem(CHAVE, cena.nome);
      } catch {
        // Sem armazenamento: só não evita a repetição.
      }
      return { cena, seed: Math.floor(Math.random() * 2 ** 31) };
    } catch (err) {
      console.error('cenas.json:', err);
      return null;
    }
  })();

  $effect(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx || !abertura) return;
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
      const { width, height } = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(width * dpr));
      canvas.height = Math.max(1, Math.round(height * dpr));
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx!.font = `${CH * 1.05}px ui-monospace, Consolas, monospace`;
      ctx!.textBaseline = 'top';
      cols = Math.ceil(width / CW);
      rows = Math.ceil(height / CH);
      glifos = new Array(cols * rows).fill('');
      cores = new Array(cols * rows).fill('');
      try {
        desenhar = prepararCena(abertura!.cena, cols, rows, semente(abertura!.seed));
      } catch (err) {
        console.error(`cenas.json, cena "${abertura!.cena.nome}":`, err);
        desenhar = null;
      }
      quadro();
    }

    const ro = new ResizeObserver(medir);
    ro.observe(canvas);
    medir();

    const timer = quieto
      ? undefined
      : setInterval(() => {
          if (pausado || document.hidden) return;
          t++;
          quadro();
        }, 100);

    return () => {
      ro.disconnect();
      clearInterval(timer);
    };
  });
</script>

<canvas bind:this={canvas} class="fundo-ascii" aria-hidden="true"></canvas>

<style>
  .fundo-ascii {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    background: #060608;
  }
</style>
