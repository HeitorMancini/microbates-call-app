import dados from './cenas/cenas.json';
import { sortearCena, validarCenas, type Cena } from './cenas/motor';

const CHAVE_CENA = 'sheeshcord:ultima-cena';
const CHAVE_LIGADO = 'sheeshcord:fundo-ascii';

function ler(chave: string): string | null {
  try {
    return localStorage.getItem(chave);
  } catch {
    return null;
  }
}

function gravar(chave: string, valor: string) {
  try {
    localStorage.setItem(chave, valor);
  } catch {
    // Sem armazenamento: só não persiste.
  }
}

export interface Abertura {
  cena: Cena;
  seed: number;
}

function sortear(anterior: string | null): Abertura | null {
  try {
    const cena = sortearCena(validarCenas(dados), anterior);
    gravar(CHAVE_CENA, cena.nome);
    return { cena, seed: Math.floor(Math.random() * 2 ** 31) };
  } catch (err) {
    console.error('cenas.json:', err);
    return null;
  }
}

// Fundo único do app (entrada e chamada): a cena sobrevive à troca de tela.
export class FundoStore {
  ligado = $state(ler(CHAVE_LIGADO) !== 'off');
  abertura = $state<Abertura | null>(sortear(ler(CHAVE_CENA)));

  embaralhar() {
    this.abertura = sortear(this.abertura?.cena.nome ?? null);
  }

  alternar(ligado: boolean) {
    this.ligado = ligado;
    gravar(CHAVE_LIGADO, ligado ? 'on' : 'off');
  }
}

export const fundo = new FundoStore();
