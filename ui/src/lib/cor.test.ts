import { blobatar } from 'blobatar';
import { describe, expect, it } from 'vitest';
import { corDoBlob, destaqueDe, tom } from './cor';

describe('destaqueDe', () => {
  it.each([
    ['heitor', '#FBB60B'],
    ['joao', '#0779FE'],
    ['gabi', '#B907FB'],
    ['lucas', '#FB5505'],
  ])('leva o tom do blob de %s ao ponto mais vivo do matiz', (nome, esperado) => {
    expect(destaqueDe(corDoBlob(blobatar(nome))).cor).toBe(esperado);
  });

  it('escolhe o texto pelo contraste', () => {
    expect(destaqueDe('#f0d441').texto).toBe('#0b0b0c');
    expect(destaqueDe('#3a2bd6').texto).toBe('#ffffff');
  });

  it('gera variações dentro do sRGB', () => {
    expect(tom(destaqueDe('#bad6ff'), 0.15, 0.6)).toMatch(/^#[0-9A-F]{6}$/);
  });
});
