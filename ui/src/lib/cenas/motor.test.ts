import { describe, expect, it } from 'vitest';
import dados from './cenas.json';
import { prepararCena, semente, sortearCena, validarCenas, type Cena } from './motor';

const cenas = validarCenas(dados);

describe('cenas.json', () => {
  it.each(cenas.map((c) => [c.nome, c] as const))('"%s" desenha dentro da grade', (_, cena) => {
    const cols = 60;
    const rows = 60;
    const desenhar = prepararCena(cena, cols, rows, semente(42));
    let visiveis = 0;
    for (const t of [0, 17]) {
      desenhar((x, y, ch) => {
        if (ch !== ' ') visiveis++;
        expect(ch).toHaveLength(1);
        expect(Number.isFinite(x) && Number.isFinite(y)).toBe(true);
      }, t);
    }
    expect(visiveis).toBeGreaterThan(200);
  });
});

describe('sortearCena', () => {
  it('nunca repete a cena anterior', () => {
    const rnd = semente(7);
    let anterior: string | null = null;
    for (let i = 0; i < 40; i++) {
      const cena = sortearCena(cenas, anterior, rnd);
      expect(cena.nome).not.toBe(anterior);
      anterior = cena.nome;
    }
  });

  it('com uma única cena, repete', () => {
    const unica: Cena[] = [cenas[0]];
    expect(sortearCena(unica, unica[0].nome).nome).toBe(unica[0].nome);
  });
});

describe('validarCenas', () => {
  it('aponta o tipo de camada desconhecido', () => {
    expect(() => validarCenas({ cenas: [{ nome: 'X', camadas: [{ tipo: 'chuva' }] }] })).toThrow(
      /cena 1 \("X"\), camada 1: tipo "chuva" desconhecido/,
    );
  });

  it('aponta valor relativo ao horizonte malformado', () => {
    const cena: Cena = { nome: 'X', camadas: [{ tipo: 'nevoa', y: 'horizont', cor: '#ffffff' }] };
    expect(() => prepararCena(cena, 10, 10, semente(1))).toThrow(/Valor inválido: "horizont"/);
  });
});
