import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import { RoomStore } from './room.svelte';

const join = { serverUrl: ' ws://26.1.2.3:7880 ', room: ' resenha ', name: 'Ana' };

function makeStore() {
  return new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
}

describe('RoomStore', () => {
  it('entra, expõe participantes e o local', async () => {
    const store = makeStore();
    expect(await store.join(join)).toBe(true);
    expect(store.status).toBe('connected');
    expect(store.inRoom).toBe(true);
    expect(store.roomName).toBe('resenha');
    expect(store.local?.name).toBe('Ana');
    expect(store.participants.length).toBeGreaterThan(1);
    expect(store.announcement).toMatch(/entrou na sala resenha/);
  });

  it('erro de entrada vira mensagem e não entra', async () => {
    const store = makeStore();
    expect(await store.join({ ...join, serverUrl: 'x' })).toBe(false);
    expect(store.inRoom).toBe(false);
    expect(store.error).toMatch(/https:\/\/live\.microbates\.com/);
    store.dismissError();
    expect(store.error).toBeNull();
    expect(store.status).toBe('disconnected');
  });

  it('alterna mudo e anuncia', async () => {
    const store = makeStore();
    await store.join(join);
    await store.toggleMute();
    expect(store.muted).toBe(true);
    expect(store.announcement).toBe('Microfone desligado.');
    await store.toggleMute();
    expect(store.muted).toBe(false);
  });

  it('ensurdecer também muta; desmutar tira o ensurdecer', async () => {
    const store = makeStore();
    await store.join(join);
    await store.toggleDeafen();
    expect(store.deafened).toBe(true);
    expect(store.muted).toBe(true);
    await store.toggleMute();
    expect(store.muted).toBe(false);
    expect(store.deafened).toBe(false);
  });

  it('carrega e seleciona dispositivos', async () => {
    const store = makeStore();
    await store.loadDevices();
    expect(store.devices.inputs.length).toBeGreaterThan(0);
    await store.selectDevice('output', 'headset');
    expect(store.selectedOutput).toBe('headset');
    await store.selectDevice('output', 'nada');
    expect(store.selectedOutput).toBe('headset');
    expect(store.error).toMatch(/não encontrado/);
  });

  it('compartilha tela e aparece entre quem transmite', async () => {
    const store = makeStore();
    await store.join(join);
    await store.loadScreenSources();
    expect(await store.startShare(store.screenSources[0].id, '1080p60')).toBe(true);
    expect(store.sharing).toBe(true);
    expect(store.sharers.map((p) => p.isLocal)).toEqual([true]);
    expect(store.sharePreset).toBe('1080p60');
    await store.stopShare();
    expect(store.sharing).toBe(false);
  });

  it('sair limpa a sala', async () => {
    const store = makeStore();
    await store.join(join);
    await store.toggleDeafen();
    await store.leave();
    expect(store.status).toBe('disconnected');
    expect(store.participants).toEqual([]);
    expect(store.deafened).toBe(false);
    expect(store.inRoom).toBe(false);
  });
});