import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from '../lib/mock';
import { RoomStore } from '../lib/room.svelte';
import DeviceSettings from './DeviceSettings.svelte';

function makeStore(level: () => Promise<number> = async () => -30) {
  const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false });
  backend.getMicLevel = vi.fn(level);
  backend.setNoiseSuppression = vi.fn().mockResolvedValue(undefined);
  backend.setMicSensitivity = vi.fn().mockResolvedValue(undefined);
  return { backend, store: new RoomStore(backend) };
}

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('DeviceSettings: supressão de ruído e sensibilidade', () => {
  it('mostra a chave de supressão ligada e a desliga', async () => {
    const { backend, store } = makeStore();
    render(DeviceSettings, { store, onDone: () => {} });
    const chave = screen.getByRole('checkbox', { name: 'Supressão de ruído' }) as HTMLInputElement;
    expect(chave.checked).toBe(true);
    await fireEvent.click(chave);
    await vi.advanceTimersByTimeAsync(0);
    expect(backend.setNoiseSuppression).toHaveBeenCalledWith(false);
    expect(store.noiseSuppression).toBe(false);
  });

  it('o slider de sensibilidade mostra 100 como "sem corte" e envia o novo valor', async () => {
    const { backend, store } = makeStore();
    render(DeviceSettings, { store, onDone: () => {} });
    expect(screen.getByText('Sensibilidade do microfone: sem corte')).toBeInTheDocument();
    const slider = screen.getByRole('slider', { name: /Sensibilidade do microfone/ }) as HTMLInputElement;
    expect(slider.max).toBe('100');
    slider.value = '40';
    slider.dispatchEvent(new Event('input', { bubbles: true }));
    await vi.advanceTimersByTimeAsync(0);
    expect(backend.setMicSensitivity).toHaveBeenCalledWith(40);
    expect(screen.getByText('Sensibilidade do microfone: 40%')).toBeInTheDocument();
  });

  it('o medidor expõe o nível e a marca do limiar só abaixo de 100', async () => {
    const { store } = makeStore(async () => -30);
    render(DeviceSettings, { store, onDone: () => {} });
    await vi.advanceTimersByTimeAsync(100);
    const medidor = screen.getByRole('meter', { name: 'Nível do microfone' });
    expect(medidor).toHaveAttribute('aria-valuemin', '-60');
    expect(medidor).toHaveAttribute('aria-valuemax', '0');
    expect(medidor).toHaveAttribute('aria-valuenow', '-30');
    expect(document.querySelector('[data-limiar]')).toBeNull();

    await store.setMicSensitivity(50);
    await vi.advanceTimersByTimeAsync(0);
    expect(document.querySelector('[data-limiar]')).not.toBeNull();
  });

  it('consulta o nível enquanto aberto e para ao fechar', async () => {
    const { backend, store } = makeStore();
    const { unmount } = render(DeviceSettings, { store, onDone: () => {} });
    await vi.advanceTimersByTimeAsync(400);
    const durante = vi.mocked(backend.getMicLevel).mock.calls.length;
    expect(durante).toBeGreaterThan(3);
    unmount();
    await vi.advanceTimersByTimeAsync(400);
    expect(vi.mocked(backend.getMicLevel).mock.calls.length).toBe(durante);
  });

  it('se o nível falha, o medidor fica sem sinal e nenhum erro aparece', async () => {
    const { store } = makeStore(async () => {
      throw 'sem áudio';
    });
    render(DeviceSettings, { store, onDone: () => {} });
    await vi.advanceTimersByTimeAsync(200);
    expect(screen.getByRole('meter', { name: 'Nível do microfone' })).toHaveAttribute('aria-valuenow', '-60');
    expect(store.error).toBeNull();
  });
});
