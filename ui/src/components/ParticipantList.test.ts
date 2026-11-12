import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { Participant } from '../lib/types';
import ParticipantList from './ParticipantList.svelte';

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

describe('ParticipantList', () => {
  it('lista todos com estado legível por leitor de tela', () => {
    render(ParticipantList, {
      participants: [
        p({ name: 'Ana', isLocal: true, speaking: true }),
        p({ name: 'Caio', muted: true }),
        p({ name: 'Bia', sharingScreen: true }),
      ],
    });
    const list = screen.getByRole('list', { name: 'Pessoas na sala (3)' });
    const items = within(list).getAllByRole('listitem');
    expect(items.map(spokenText)).toEqual([
      'Ana: você, falando',
      'Caio: microfone desligado',
      'Bia: compartilhando tela',
    ]);
    expect(items[0]).toHaveAttribute('data-speaking', 'true');
    expect(items[1]).toHaveAttribute('data-speaking', 'false');
  });

  it('mostra convite quando vazio', () => {
    render(ParticipantList, { participants: [] });
    expect(screen.queryByRole('list')).toBeNull();
    expect(screen.getByText(/Ninguém por aqui/)).toBeInTheDocument();
  });
});