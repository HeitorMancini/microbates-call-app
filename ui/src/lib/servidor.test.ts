import { describe, expect, it } from 'vitest';
import { tokenEndpoint } from './servidor';

describe('tokenEndpoint', () => {
  it('converte o esquema do servidor para o endpoint HTTP de token', () => {
    expect(tokenEndpoint('wss://h')).toBe('https://h/token');
    expect(tokenEndpoint('ws://h:1/')).toBe('http://h:1/token');
    expect(tokenEndpoint('http://h:1')).toBe('http://h:1/token');
    expect(tokenEndpoint('h:7881')).toBe('http://h:7881/token');
    expect(tokenEndpoint('wss://live.microbates.com:8443')).toBe('https://live.microbates.com:8443/token');
  });
});
