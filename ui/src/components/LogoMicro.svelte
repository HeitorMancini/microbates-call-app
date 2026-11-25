<script lang="ts">
  import { desenharMu, rasterizarMu } from '../lib/logo';

  // μ em ASCII na cor de destaque + "Chat" na fonte do app, alinhados na linha de base.
  let { tamanho = 100 }: { tamanho?: number } = $props();

  let canvas: HTMLCanvasElement;

  $effect(() => {
    const px = tamanho;
    const cw = Math.max(3, Math.round((px * 5) / 136));
    const ch = Math.round(cw * 1.5);
    const chatPx = Math.round(px * 0.34);
    const fonteChat = `800 ${chatPx}px 'Bricolage Grotesque Variable', system-ui, sans-serif`;
    const reduz = matchMedia('(prefers-reduced-motion: reduce)').matches;
    let timer: ReturnType<typeof setInterval> | undefined;
    let vivo = true;

    const fontes = document.fonts ? document.fonts.load(fonteChat).catch(() => undefined) : Promise.resolve();
    void fontes.then(() => {
      if (!vivo) return;
      const g = rasterizarMu(px, cw, ch);
      const ctx = canvas.getContext('2d');
      if (!g || !ctx) return;
      ctx.font = fonteChat;
      const gap = Math.round(-px * 0.045);
      const largura = Math.ceil(g.w + gap + ctx.measureText('Chat').width + 4);
      const dpr = devicePixelRatio || 1;
      canvas.width = largura * dpr;
      canvas.height = g.h * dpr;
      canvas.style.width = `${largura}px`;
      canvas.style.height = `${g.h}px`;

      const quadro = () => {
        if (document.hidden) return;
        const raiz = getComputedStyle(document.documentElement);
        const cor = raiz.getPropertyValue('--color-valvula').trim() || '#6a6a72';
        const giz = raiz.getPropertyValue('--color-giz').trim() || '#ececf0';
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, largura, g.h);
        desenharMu(ctx, g, cw, ch, cor, reduz ? 0 : 0.035);
        ctx.font = fonteChat;
        ctx.textBaseline = 'alphabetic';
        ctx.fillStyle = giz;
        ctx.fillText('Chat', g.w + gap, g.base);
      };
      quadro();
      timer = setInterval(quadro, 120);
    });

    return () => {
      vivo = false;
      clearInterval(timer);
    };
  });
</script>

<canvas bind:this={canvas} class="logo" aria-hidden="true"></canvas>

<style>
  .logo {
    display: block;
    filter: drop-shadow(0 2px 12px rgb(0 0 0 / 0.55));
  }
</style>
