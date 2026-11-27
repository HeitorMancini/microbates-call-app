<script lang="ts">
  import { blobatar } from 'blobatar';
  import { corDoBlob, destaqueDe, type Destaque } from '../lib/cor';
  import { arrastavel } from '../lib/janela';
  import type { RoomStore } from '../lib/room.svelte';
  import { aplicarDestaque } from '../lib/tema';
  import { DEFAULT_ROOM, DEFAULT_SERVER_URL } from '../lib/types';
  import { hasErrors, MAX_FIELD_LEN, validateJoin } from '../lib/validation';
  import BotaoEntrar from './BotaoEntrar.svelte';
  import { fundo } from '../lib/fundo.svelte';
  import Icon from './Icon.svelte';
  import LogoMicro from './LogoMicro.svelte';

  const STORAGE_KEY = 'sheeshcord:entrada';
  const ESPERA_MS = 500;

  let { store, saindo = false }: { store: RoomStore; saindo?: boolean } = $props();

  function loadSaved(): { name: string; password: string } {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const { name = '', password = '' } = JSON.parse(raw);
        return { name: String(name), password: String(password) };
      }
    } catch {
      // Sem armazenamento: começa vazio.
    }
    return { name: '', password: '' };
  }

  function gerar(nome: string): { svg: string; destaque: Destaque } | null {
    const limpo = nome.trim();
    if (!limpo) return null;
    const svg = blobatar(limpo);
    return { svg, destaque: destaqueDe(corDoBlob(svg)) };
  }

  const salvo = loadSaved();
  let name = $state(salvo.name);
  let password = $state(salvo.password);
  let blob = $state(gerar(salvo.name));
  let esperando = $state(false);
  let erroNome = $state<string | null>(null);
  let campoNome: HTMLInputElement;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const destaque = $derived(esperando ? null : (blob?.destaque ?? null));
  const busy = $derived(store.busy || store.status === 'connecting');

  $effect(() => aplicarDestaque(destaque));
  $effect(() => () => clearTimeout(timer));

  function confirmarNome() {
    clearTimeout(timer);
    blob = gerar(name);
    esperando = false;
  }

  function digitou() {
    esperando = true;
    erroNome = null;
    clearTimeout(timer);
    timer = setTimeout(confirmarNome, ESPERA_MS);
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    confirmarNome();
    const form = { serverUrl: DEFAULT_SERVER_URL, room: DEFAULT_ROOM, name, password };
    const errors = validateJoin(form);
    if (hasErrors(errors)) {
      erroNome = errors.name ?? null;
      campoNome.focus();
      return;
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ name, password }));
    } catch {
      // Ignora: salvar é só conveniência.
    }
    await store.join(form);
  }
</script>

<main class="entrada" class:saindo use:arrastavel>
  <div class="fosco" aria-hidden="true"></div>

  <button type="button" class="embaralhar" aria-label="Trocar fundo" title="Trocar fundo" onclick={() => fundo.embaralhar()}>
    <Icon name="shuffle" size={16} />
  </button>

  <form class="card" novalidate onsubmit={submit} aria-labelledby="titulo-app">
    <h1 id="titulo-app" class="titulo">
      <span class="sr-only">μChat</span>
      <LogoMicro tamanho={130} />
    </h1>

    <div class="palco">
      <div class="avatar" class:esperando aria-hidden="true">
      {#if blob}
        {#key blob.svg}
          <img class="blob" alt="" src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(blob.svg)}`} />
        {/key}
      {:else}
        <span class="vazio"></span>
      {/if}
      </div>
    </div>

    <div class="campos">
    <label class="sr-only" for="campo-nome">Nome</label>
    <input
      bind:this={campoNome}
      id="campo-nome"
      class="campo-entrada"
      name="name"
      type="text"
      placeholder="Nome"
      autocomplete="off"
      spellcheck="false"
      maxlength={MAX_FIELD_LEN + 16}
      bind:value={name}
      oninput={digitou}
      aria-invalid={erroNome ? 'true' : undefined}
      aria-describedby={erroNome ? 'nome-erro' : undefined}
      required
    />
    {#if erroNome}
      <p class="erro" id="nome-erro">
        <Icon name="alert" size={14} />
        {erroNome}
      </p>
    {/if}

    <label class="sr-only" for="campo-senha">Senha da sala</label>
    <input
      id="campo-senha"
      class="campo-entrada"
      name="password"
      type="password"
      placeholder="Senha da sala"
      autocomplete="current-password"
      bind:value={password}
    />

    {#if store.error}
      <p class="falha" role="alert">
        <Icon name="alert" size={16} />
        <span>Não deu para entrar: {store.error}</span>
      </p>
    {/if}

    <div class="acao">
      <BotaoEntrar {destaque} ocupado={busy} rotulo={busy ? 'Conectando…' : 'Entrar'} />
    </div>
    </div>
  </form>
</main>

<style>
  .entrada {
    position: relative;
    height: 100%;
    overflow: hidden;
  }

  .embaralhar {
    position: absolute;
    top: 10px;
    left: 12px;
    z-index: 6;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    border: 1px solid rgb(255 255 255 / 0.12);
    background: rgb(8 8 10 / 0.55);
    color: var(--color-nevoa);
    transition:
      background-color 160ms ease,
      color 160ms ease;
  }

  .embaralhar:hover {
    background: rgb(8 8 10 / 0.85);
    color: var(--color-giz);
  }

  .fosco {
    position: absolute;
    inset: 0;
    backdrop-filter: blur(1.5px);
    transition: opacity 380ms ease;
  }

  /* Ao entrar na chamada: só o card se dissolve em zoom; o fundo continua. */
  .saindo .fosco {
    opacity: 0;
  }

  .card {
    position: relative;
    height: 100%;
    box-sizing: border-box;
    padding: 40px 32px 32px;
    display: flex;
    flex-direction: column;
    align-items: center;
    transition:
      opacity 300ms ease,
      transform 380ms var(--ease-saida);
  }

  .saindo .card {
    opacity: 0;
    transform: scale(1.06);
  }

  .titulo {
    margin: 0;
    line-height: 0;
  }

  /* O blob fica no espaço entre a marca e o formulário, um pouco mais perto da marca. */
  .palco {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 112px;
    padding-bottom: 44px;
  }

  .avatar {
    width: 96px;
    height: 96px;
    display: grid;
    place-items: center;
  }

  .campos {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .blob {
    display: block;
    width: 96px;
    height: 96px;
    animation: brotar 350ms var(--ease-mola);
    transition:
      opacity 300ms,
      filter 300ms,
      transform 300ms;
  }



  .esperando .blob {
    opacity: 0.3;
    filter: grayscale(1);
    transform: scale(0.92);
  }

  @keyframes brotar {
    from {
      transform: scale(0.7);
      opacity: 0;
    }
  }

  .vazio {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    border: 2px dashed #3a3a40;
  }

  .campo-entrada {
    width: 100%;
    min-height: 44px;
    padding: 0 14px;
    box-sizing: border-box;
    border-radius: 9px;
    border: 1px solid rgb(255 255 255 / 0.12);
    background: rgb(8 8 10 / 0.55);
    color: var(--color-giz);
    font-size: 15px;
    outline: none;
    transition:
      border-color 300ms,
      box-shadow 300ms;
  }

  .campo-entrada::placeholder {
    color: #85858d;
  }

  .campo-entrada:focus-visible {
    border-color: var(--color-valvula);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-valvula) 25%, transparent);
  }

  .campo-entrada[aria-invalid='true'] {
    border-color: var(--color-corte);
  }

  .erro,
  .falha {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    align-self: stretch;
    margin: -6px 0 0;
    font-size: 13px;
    color: var(--color-corte);
  }

  .falha {
    margin: 0;
    padding: 8px 10px;
    border-radius: 9px;
    background: rgb(20 8 10 / 0.75);
    color: var(--color-giz);
  }

  .falha :global(svg),
  .erro :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--color-corte);
  }

  .acao {
    margin-top: 8px;
    width: 100%;
  }
</style>
