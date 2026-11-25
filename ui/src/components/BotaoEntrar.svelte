<script lang="ts">
  import { tom, type Destaque } from '../lib/cor';
  import type { Brilho } from '../lib/brilho';

  let {
    destaque,
    ocupado = false,
    rotulo = 'Entrar',
  }: { destaque: Destaque | null; ocupado?: boolean; rotulo?: string } = $props();

  let ilha: HTMLSpanElement;
  let hover = $state(false);
  let pressionado = $state(false);
  let brilho = $state<Brilho | null>(null);

  const ativo = $derived(destaque !== null);
  const cores = $derived(
    destaque ? [tom(destaque, 0.08, 0.9), destaque.cor, tom(destaque, -0.08, 1), tom(destaque, 0.15, 0.6)] : undefined,
  );

  // O voice-glow desenha em canvas; no jsdom (testes) não há canvas, então fica de fora.
  $effect(() => {
    if (navigator.userAgent.includes('jsdom')) return;
    let vivo = true;
    let montado: Brilho | null = null;
    void import('../lib/brilho').then(({ montarBrilho }) => {
      if (!vivo) return;
      montado = montarBrilho(ilha);
      brilho = montado;
    });
    return () => {
      vivo = false;
      montado?.destruir();
      brilho = null;
    };
  });

  $effect(() => {
    brilho?.atualizar(ativo && hover && !ocupado, cores);
  });
</script>

<button
  type="submit"
  class="entrar"
  class:ativo
  class:hover={ativo && hover}
  class:pressionado={ativo && pressionado}
  aria-busy={ocupado}
  style:--a={destaque?.cor}
  style:--at={destaque?.texto}
  onpointerenter={() => (hover = true)}
  onpointerleave={() => {
    hover = false;
    pressionado = false;
  }}
  onpointerdown={() => (pressionado = true)}
  onpointerup={() => (pressionado = false)}
>
  <span class="fundo"></span>
  <span class="ilha" bind:this={ilha} aria-hidden="true"></span>
  <span class="rotulo">{rotulo}</span>
</button>

<style>
  /* Inativo: menor e quase mesclado ao fundo. Ao definir a cor: zoom com mola + fade. */
  .entrar {
    --s: 0.84;
    position: relative;
    display: block;
    width: 100%;
    height: 48px;
    padding: 0;
    border: none;
    border-radius: 10px;
    background: none;
    font-weight: 700;
    font-size: 15px;
    cursor: default;
    transform: scale(var(--s));
    transition: transform 320ms var(--ease-saida);
  }

  .entrar.ativo {
    --s: 1;
    cursor: pointer;
    transition: transform 550ms var(--ease-mola);
  }

  .entrar.hover {
    --s: 1.045;
    transition: transform 350ms var(--ease-mola);
  }

  .entrar.pressionado {
    --s: 0.97;
    transition-duration: 100ms;
  }

  .fundo {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    border: 1px solid rgb(255 255 255 / 0.07);
    background-color: rgb(255 255 255 / 0.05);
    transition:
      background-color 320ms ease,
      border-color 320ms ease;
  }

  .ativo .fundo {
    border-color: transparent;
    background-color: var(--a);
    transition:
      background-color 450ms ease,
      border-color 450ms ease;
  }

  /* Hover: o botão escurece e o brilho pixelado na cor do usuário sobe da borda. */
  .hover .fundo {
    background-color: #111114;
    border-color: color-mix(in srgb, var(--a) 35%, transparent);
  }

  .ilha {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    pointer-events: none;
  }

  .rotulo {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    color: rgb(236 236 240 / 0.38);
    text-shadow: 0 1px 6px rgb(0 0 0 / 0.35);
    pointer-events: none;
    transition: color 400ms ease;
  }

  .ativo .rotulo {
    color: var(--at);
    text-shadow: none;
  }

  .hover .rotulo {
    color: #fff;
    text-shadow: 0 1px 8px rgb(0 0 0 / 0.6);
  }
</style>
