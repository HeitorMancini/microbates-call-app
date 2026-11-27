import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockBackend } from './mock';
import { RoomStore } from './room.svelte';

const join = { serverUrl: ' ws://26.1.2.3:7880 ', room: ' resenha ', name: 'Ana' };

function makeStore() {
  return new RoomStore(createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false }));
}

beforeEach(() => localStorage.clear());

describe('RoomStore: processamento do microfone', () => {
  it('começa com supressão ligada e sensibilidade 100 (gate aberto)', () => {
    const store = makeStore();
    expect(store.noiseSuppression).toBe(true);
    expect(store.micSensitivity).toBe(100);
  });

  it('limita a sensibilidade a 0–100 e arredonda', async () => {
    const store = makeStore();
    await store.setMicSensitivity(42.6);
    expect(store.micSensitivity).toBe(43);
    await store.setMicSensitivity(250);
    expect(store.micSensitivity).toBe(100);
    await store.setMicSensitivity(-3);
    expect(store.micSensitivity).toBe(0);
  });

  it('só muda o estado quando o backend aceita', async () => {
    const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false });
    backend.setNoiseSuppression = vi.fn().mockRejectedValue('falhou');
    backend.setMicSensitivity = vi.fn().mockRejectedValue('falhou');
    const store = new RoomStore(backend);
    await store.setNoiseSuppression(false);
    await store.setMicSensitivity(10);
    expect(store.noiseSuppression).toBe(true);
    expect(store.micSensitivity).toBe(100);
    expect(store.error).toBe('falhou');
  });

  it('persiste os ajustes e os reenvia ao backend ao iniciar', async () => {
    const first = makeStore();
    await first.setNoiseSuppression(false);
    await first.setMicSensitivity(40);
    await first.setMicVolume(130);

    const backend = createMockBackend({ latencyMs: 0, autoSpeak: false, frames: false });
    backend.setNoiseSuppression = vi.fn().mockResolvedValue(undefined);
    backend.setMicSensitivity = vi.fn().mockResolvedValue(undefined);
    backend.setMicVolume = vi.fn().mockResolvedValue(undefined);
    const second = new RoomStore(backend);
    await second.start();
    expect(backend.setNoiseSuppression).toHaveBeenCalledWith(false);
    expect(backend.setMicSensitivity).toHaveBeenCalledWith(40);
    expect(backend.setMicVolume).toHaveBeenCalledWith(130);
    expect(second.noiseSuppression).toBe(false);
    expect(second.micSensitivity).toBe(40);
    expect(second.micVolume).toBe(130);
    second.stop();
  });

  it('JSON corrompido ou valores inválidos caem nos padrões sem lançar', async () => {
    localStorage.setItem('sheeshcord:audio', '{nao e json');
    const a = makeStore();
    await a.start();
    expect(a.micSensitivity).toBe(100);
    a.stop();

    localStorage.setItem(
      'sheeshcord:audio',
      JSON.stringify({ noiseSuppression: 'sim', micSensitivity: 'x', micVolume: null }),
    );
    const b = makeStore();
    await b.start();
    expect(b.noiseSuppression).toBe(true);
    expect(b.micSensitivity).toBe(100);
    expect(b.micVolume).toBe(100);
    b.stop();
  });
});

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

  it('ajusta o volume do microfone limitando a 0–200%', async () => {
    const store = makeStore();
    expect(store.micVolume).toBe(100);
    await store.setMicVolume(150);
    expect(store.micVolume).toBe(150);
    await store.setMicVolume(500);
    expect(store.micVolume).toBe(200);
    await store.setMicVolume(-5);
    expect(store.micVolume).toBe(0);
  });

  it('guarda o volume de cada pessoa (0–200) e assume 100 por padrão', async () => {
    const store = makeStore();
    expect(store.volumeOf('ana')).toBe(100);
    await store.setParticipantVolume('ana', 40);
    await store.setParticipantVolume('bia', 900);
    expect(store.volumeOf('ana')).toBe(40);
    expect(store.volumeOf('bia')).toBe(200);
    expect(store.volumeOf('caio')).toBe(100);
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

  it('troca de sala sem sair da call e mantém nome e senha', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1', password: 's' });
    const visto: boolean[] = [];
    const parar = $effect.root(() => {
      $effect(() => {
        visto.push(store.presente);
      });
    });
    expect(await store.trocarSala('Call 2')).toBe(true);
    parar();
    expect(store.roomName).toBe('Call 2');
    expect(store.local?.name).toBe('Ana');
    expect(visto.every(Boolean)).toBe(true);
    expect(store.trocando).toBe(false);
  });

  it('chega mudo na nova sala se estava mudo', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    await store.toggleMute();
    expect(await store.trocarSala('Call 2')).toBe(true);
    expect(store.muted).toBe(true);
  });

  it('chega ensurdecido e mudo se estava ensurdecido', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    await store.toggleDeafen();
    expect(await store.trocarSala('Call 2')).toBe(true);
    expect(store.deafened).toBe(true);
    expect(store.muted).toBe(true);
  });

  it('compartilhamento para ao trocar e é anunciado', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    await store.loadScreenSources();
    await store.startShare(store.screenSources[0].id, '1080p30');
    expect(await store.trocarSala('Call 2')).toBe(true);
    expect(store.sharing).toBe(false);
    expect(store.announcement).toMatch(/Compartilhamento encerrado ao trocar de sala/);
  });

  it('falha na troca mantém a call aberta com o erro; sair limpa', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    expect(await store.trocarSala('   ')).toBe(false);
    expect(store.falhaTroca).toBe(true);
    expect(store.presente).toBe(true);
    expect(store.error).not.toBeNull();
    expect(await store.trocarSala('Call 3')).toBe(true);
    expect(store.falhaTroca).toBe(false);
    await store.trocarSala('   ');
    await store.leave();
    expect(store.falhaTroca).toBe(false);
    expect(store.presente).toBe(false);
  });

  it('falha do backend ao sair da sala não propaga: resolve false e marca erro', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    const original = store.backend.leaveRoom.bind(store.backend);
    store.backend.leaveRoom = () => Promise.reject(new Error('leave_room falhou'));
    expect(await store.trocarSala('Call 2')).toBe(false);
    expect(store.falhaTroca).toBe(true);
    expect(store.presente).toBe(true);
    expect(store.error).not.toBeNull();
    expect(store.trocando).toBe(false);
    store.backend.leaveRoom = original;
  });

  it('escolher a sala atual não faz nada', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    expect(await store.trocarSala('Call 1')).toBe(true);
    expect(store.status).toBe('connected');
  });

  it('troca durante troca é ignorada', async () => {
    const store = makeStore();
    await store.join({ ...join, room: 'Call 1' });
    const a = store.trocarSala('Call 2');
    const b = store.trocarSala('Call 3');
    expect(await b).toBe(false);
    expect(await a).toBe(true);
    expect(store.roomName).toBe('Call 2');
  });
});