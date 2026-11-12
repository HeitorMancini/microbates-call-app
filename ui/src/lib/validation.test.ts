import { describe, expect, it } from 'vitest';
import { hasErrors, normalizeJoin, validateJoin } from './validation';

const ok = { serverUrl: 'ws://26.1.2.3:7880', room: 'resenha', name: 'Ana' };

describe('validateJoin', () => {
  it('aceita entrada válida', () => {
    expect(hasErrors(validateJoin(ok))).toBe(false);
    expect(hasErrors(validateJoin({ ...ok, serverUrl: 'https://voz.exemplo' }))).toBe(false);
  });

  it('exige todos os campos (ignorando espaços)', () => {
    const errors = validateJoin({ serverUrl: ' ', room: '', name: '   ' });
    expect(Object.keys(errors).sort()).toEqual(['name', 'room', 'serverUrl']);
  });

  it.each(['26.1.2.3:7880', 'ftp://x', 'ws://', 'ws:///x', 'ws://a b'])('rejeita servidor %s', (serverUrl) => {
    expect(validateJoin({ ...ok, serverUrl }).serverUrl).toMatch(/https:\/\/live\.microbates\.com/);
  });

  it('limita sala e nome a 64 caracteres (contando emoji como um)', () => {
    expect(validateJoin({ ...ok, name: 'x'.repeat(64) }).name).toBeUndefined();
    expect(validateJoin({ ...ok, name: 'x'.repeat(65) }).name).toMatch(/64/);
    expect(validateJoin({ ...ok, room: '🎮'.repeat(64) }).room).toBeUndefined();
  });

  it('normaliza espaços', () => {
    expect(normalizeJoin({ serverUrl: ' ws://h ', room: ' s ', name: ' n ' })).toEqual({
      serverUrl: 'ws://h',
      room: 's',
      name: 'n',
    });
  });
});