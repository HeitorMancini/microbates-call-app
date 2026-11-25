import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import SeletorSala from './SeletorSala.svelte';

describe('SeletorSala', () => {
  it('abre o menu com as 3 salas e a atual marcada', async () => {
    const user = userEvent.setup();
    render(SeletorSala, { atual: 'Call 1', ocupado: false, onescolher: () => {} });
    const botao = screen.getByRole('button', { name: 'Call 1' });
    expect(botao).toHaveAttribute('aria-haspopup', 'menu');
    expect(botao).toHaveAttribute('aria-expanded', 'false');

    await user.click(botao);
    expect(botao).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('menuitemradio', { name: 'Call 1' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('menuitemradio', { name: 'Call 2' })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByRole('menuitemradio', { name: 'Call 3' })).toHaveAttribute('aria-checked', 'false');
  });

  it('escolher chama onescolher e fecha', async () => {
    const user = userEvent.setup();
    const onescolher = vi.fn();
    render(SeletorSala, { atual: 'Call 1', ocupado: false, onescolher });
    await user.click(screen.getByRole('button', { name: 'Call 1' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Call 2' }));
    expect(onescolher).toHaveBeenCalledWith('Call 2');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('Escape fecha e devolve o foco ao botão', async () => {
    const user = userEvent.setup();
    render(SeletorSala, { atual: 'Call 1', ocupado: false, onescolher: () => {} });
    const botao = screen.getByRole('button', { name: 'Call 1' });
    await user.click(botao);
    expect(screen.getByRole('menu')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).toBeNull();
    expect(botao).toHaveFocus();
  });

  it('ocupado desabilita o botão', () => {
    render(SeletorSala, { atual: 'Call 1', ocupado: true, onescolher: () => {} });
    expect(screen.getByRole('button', { name: 'Call 1' })).toBeDisabled();
  });
});
