// Janela sem a barra nativa: controles próprios, arrastar e a transição de
// tamanho entre o card de entrada e a tela da chamada. Fora do Tauri, nada faz.
import { isTauri } from '@tauri-apps/api/core';

export interface Tamanho {
  width: number;
  height: number;
}

export const TAMANHO_ENTRADA: Tamanho = { width: 360, height: 540 };
export const TAMANHO_SALA: Tamanho = { width: 1100, height: 720 };
export const MINIMO_SALA: Tamanho = { width: 720, height: 520 };

const ativa = isTauri();

async function atual() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  return getCurrentWindow();
}

async function tentar(acao: () => Promise<unknown>) {
  if (!ativa) return;
  try {
    await acao();
  } catch (err) {
    console.error('janela:', err);
  }
}

export const minimizar = () => tentar(async () => (await atual()).minimize());
export const alternarMaximizar = () => tentar(async () => (await atual()).toggleMaximize());
export const fechar = () => tentar(async () => (await atual()).close());

const INTERATIVO = 'input, button, select, textarea, a, [role="button"], [data-sem-arrasto]';

/** Ação Svelte: arrastar a janela por áreas vazias do elemento. */
export function arrastavel(no: HTMLElement, opcoes: { maximizarComDuploClique?: boolean } = {}) {
  const baixo = (e: MouseEvent) => {
    if (e.button !== 0 || (e.target as Element).closest(INTERATIVO)) return;
    if (e.detail === 2 && opcoes.maximizarComDuploClique) void alternarMaximizar();
    else void tentar(async () => (await atual()).startDragging());
  };
  no.addEventListener('mousedown', baixo);
  return { destroy: () => no.removeEventListener('mousedown', baixo) };
}

const suave = (t: number) => 1 - (1 - t) ** 4;

/**
 * Leva a janela ao tamanho `alvo`, mantendo o mesmo centro (dentro do monitor).
 * Anima quadro a quadro; com movimento reduzido, troca direto.
 */
export async function animarPara(alvo: Tamanho, opcoes: { duracao?: number; minimo?: Tamanho | null; redimensionavel: boolean }) {
  await tentar(async () => {
    const { LogicalPosition, LogicalSize } = await import('@tauri-apps/api/dpi');
    const { currentMonitor } = await import('@tauri-apps/api/window');
    const w = await atual();
    if (await w.isMaximized()) await w.unmaximize();
    await w.setMinSize(null);
    await w.setResizable(true);

    const fator = await w.scaleFactor();
    const pos = (await w.outerPosition()).toLogical(fator);
    const ini = (await w.innerSize()).toLogical(fator);
    const monitor = await currentMonitor();
    const area = monitor
      ? {
          x: monitor.workArea.position.x / fator,
          y: monitor.workArea.position.y / fator,
          w: monitor.workArea.size.width / fator,
          h: monitor.workArea.size.height / fator,
        }
      : null;
    const fim = area
      ? { width: Math.min(alvo.width, area.w), height: Math.min(alvo.height, area.h) }
      : alvo;
    const cx = pos.x + ini.width / 2;
    const cy = pos.y + ini.height / 2;

    const aplicar = async (width: number, height: number) => {
      let x = cx - width / 2;
      let y = cy - height / 2;
      if (area) {
        x = Math.min(Math.max(x, area.x), area.x + area.w - width);
        y = Math.min(Math.max(y, area.y), area.y + area.h - height);
      }
      await Promise.all([w.setPosition(new LogicalPosition(Math.round(x), Math.round(y))), w.setSize(new LogicalSize(Math.round(width), Math.round(height)))]);
    };

    const duracao = matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : (opcoes.duracao ?? 460);
    const t0 = performance.now();
    for (;;) {
      const t = duracao ? Math.min(1, (performance.now() - t0) / duracao) : 1;
      const e = suave(t);
      await aplicar(ini.width + (fim.width - ini.width) * e, ini.height + (fim.height - ini.height) * e);
      if (t >= 1) break;
      await new Promise(requestAnimationFrame);
    }

    if (opcoes.minimo) await w.setMinSize(new LogicalSize(opcoes.minimo.width, opcoes.minimo.height));
    await w.setResizable(opcoes.redimensionavel);
  });
}
