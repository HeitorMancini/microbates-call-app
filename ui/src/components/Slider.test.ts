import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import Slider from './Slider.svelte';

describe('Slider', () => {
  it('desenha traços até o valor e pontos depois', () => {
    render(Slider, { id: 's', value: 100, min: 0, max: 200, step: 5 });
    const tracos = document.querySelectorAll('[data-traco="true"]');
    const pontos = document.querySelectorAll('[data-traco="false"]');
    expect(tracos.length + pontos.length).toBe(41);
    expect(tracos.length).toBe(21);
  });

  it('continua sendo um slider acessível e repassa o valor numérico', () => {
    const oninput = vi.fn();
    render(Slider, { id: 's', value: 30, min: 0, max: 200, step: 5, oninput });
    const input = screen.getByRole('slider') as HTMLInputElement;
    expect(input.id).toBe('s');
    expect(input.value).toBe('30');
    input.value = '45';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    expect(oninput).toHaveBeenCalledWith(45);
  });
});
