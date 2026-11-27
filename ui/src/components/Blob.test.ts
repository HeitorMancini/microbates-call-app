import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { tick } from 'svelte';
import { describe, expect, it, onTestFinished, vi } from 'vitest';
import type { Participant } from '../lib/types';
import Blob from './Blob.svelte';

vi.mock('../lib/brilho', () => ({
  montarBrilho: vi.fn(() => ({ atualizar: vi.fn(), destruir: vi.fn() })),
}));

const p = (over: Partial<Participant>): Participant => ({
  id: over.name ?? 'x',
  name: 'x',
  muted: false,
  speaking: false,
  sharingScreen: false,
  isLocal: false,
  ...over,
});

// Texto que um leitor de tela leria: ignora o que está em aria-hidden.
function spokenText(el: Element): string {
  const clone = el.cloneNode(true) as Element;
  clone.querySelectorAll('[aria-hidden="true"]').forEach((n) => n.remove());
  return (clone.textContent ?? '').replace(/\s+/g, ' ').trim();
}

const nivel = () => 0.6;

describe('Blob: ajuste de volume', () => {
  it('botão direito chama oncontexto', async () => {
    const oncontexto = vi.fn();
    render(Blob, { participant: p({ name: 'Ana' }), pequeno: false, assistindo: false, nivel, oncontexto });
    await userEvent.pointer({ keys: '[MouseRight]', target: screen.getByTestId('participante') });
    expect(oncontexto).toHaveBeenCalledOnce();
  });

  it('toque longo (500 ms) chama oncontexto; toque curto não', () => {
    vi.useFakeTimers();
    onTestFinished(() => {
      vi.useRealTimers();
    });
    const toque = (tipo: string) =>
      Object.assign(new MouseEvent(tipo, { bubbles: true }), { pointerType: 'touch' });
    const oncontexto = vi.fn();
    render(Blob, { participant: p({ name: 'Ana' }), pequeno: false, assistindo: false, nivel, oncontexto });
    const item = screen.getByTestId('participante');
    item.dispatchEvent(toque('pointerdown'));
    vi.advanceTimersByTime(200);
    item.dispatchEvent(toque('pointerup'));
    vi.advanceTimersByTime(600);
    expect(oncontexto).not.toHaveBeenCalled();
    item.dispatchEvent(toque('pointerdown'));
    vi.advanceTimersByTime(500);
    expect(oncontexto).toHaveBeenCalledOnce();
  });

  it('você mesmo não abre o ajuste', async () => {
    const oncontexto = vi.fn();
    render(Blob, { participant: p({ name: 'Ana', isLocal: true }), pequeno: false, assistindo: false, nivel, oncontexto });
    await userEvent.pointer({ keys: '[MouseRight]', target: screen.getByTestId('participante') });
    expect(oncontexto).not.toHaveBeenCalled();
  });
});
describe('Blob', () => {
  it('leitor de tela ouve nome e estado (mesmo texto de antes)', () => {
    render(Blob, {
      participant: p({ name: 'Ana', isLocal: true, speaking: true }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    const item = screen.getByTestId('participante');
    expect(spokenText(item)).toBe('Ana: você, falando');
  });

  it('mostra o blob animado (motion always) do nome', () => {
    render(Blob, { participant: p({ name: 'Ana' }), pequeno: false, assistindo: false, nivel });
    const item = screen.getByTestId('participante');
    expect(item.querySelector('svg.imagem')).not.toBeNull();
  });
  it('quem compartilha tem AO VIVO clicável', async () => {
    const user = userEvent.setup();
    const onassistir = vi.fn();
    render(Blob, {
      participant: p({ name: 'Bia', sharingScreen: true }),
      pequeno: false,
      assistindo: false,
      onassistir,
      nivel,
    });
    const botao = screen.getByRole('button', { name: 'AO VIVO — assistir a tela de Bia' });
    expect(botao).toHaveAttribute('aria-pressed', 'false');
    await user.click(botao);
    expect(onassistir).toHaveBeenCalled();
  });

  it('o alvo de clique de quem compartilha é o blob inteiro, não só o selo', async () => {
    const user = userEvent.setup();
    const onassistir = vi.fn();
    render(Blob, {
      participant: p({ name: 'Caio', sharingScreen: true }),
      pequeno: false,
      assistindo: false,
      onassistir,
      nivel,
    });
    const item = screen.getByTestId('participante');
    const img = item.querySelector('.imagem')!;
    const botao = screen.getByRole('button', { name: 'AO VIVO — assistir a tela de Caio' });
    expect(botao.contains(img)).toBe(true);
    await user.click(img);
    expect(onassistir).toHaveBeenCalled();
  });

  it('sem compartilhar não há AO VIVO', () => {
    render(Blob, { participant: p({ name: 'Bia' }), pequeno: false, assistindo: false, nivel });
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('só carrega o brilho quando a pessoa fala', async () => {
    const { montarBrilho } = await import('../lib/brilho');
    const result = render(Blob, {
      participant: p({ name: 'Ana', speaking: false }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    await tick();
    expect(montarBrilho).not.toHaveBeenCalled();

    await result.rerender({
      participant: p({ name: 'Ana', speaking: true }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    await vi.waitFor(() => expect(montarBrilho).toHaveBeenCalledTimes(1));
    const atualizar = vi.mocked(montarBrilho).mock.results[0].value.atualizar;
    expect(atualizar).toHaveBeenCalledWith(true, expect.arrayContaining([expect.any(String)]));
    expect(atualizar.mock.calls[0][1]).toHaveLength(4);
  });

  it('desmontar com o import do brilho pendente não monta o brilho depois', async () => {
    const { montarBrilho } = await import('../lib/brilho');
    const result = render(Blob, {
      participant: p({ name: 'Ana', speaking: true }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    // Desmonta antes do import() (microtask) resolver: `destruido` precisa
    // impedir o montarBrilho tardio de rodar num host já removido do DOM.
    result.unmount();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(montarBrilho).not.toHaveBeenCalled();
  });

  it('uma atualização de participants com o import pendente não cancela o brilho', async () => {
    const { montarBrilho } = await import('../lib/brilho');
    const result = render(Blob, {
      participant: p({ name: 'Ana', speaking: true }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    // Novo objeto de participant (como chega a cada evento do backend) antes do
    // import() resolver: antes só isso já derrubava a flag `vivo` do closure.
    await result.rerender({
      participant: p({ name: 'Ana', speaking: true }),
      pequeno: false,
      assistindo: false,
      nivel,
    });
    await vi.waitFor(() => expect(montarBrilho).toHaveBeenCalledTimes(1));
  });

  it('mic desligado mostra selo', () => {
    render(Blob, { participant: p({ name: 'Ana', muted: true }), pequeno: false, assistindo: false, nivel });
    const item = screen.getByTestId('participante');
    expect(item).toHaveAttribute('data-muted', 'true');
    expect(item.querySelector('.selo-mudo')).not.toBeNull();
  });

  it('estilo vira o style do <li> raiz (posicionamento do Palco)', () => {
    render(Blob, {
      participant: p({ name: 'Ana' }),
      pequeno: false,
      assistindo: false,
      nivel,
      estilo: 'transform: translate(10px, 20px)',
    });
    const item = screen.getByTestId('participante');
    expect(item).toHaveAttribute('style', expect.stringContaining('translate(10px, 20px)'));
  });
});
