<script lang="ts">
  import type { RoomStore } from '../lib/room.svelte';
  import { hasErrors, MAX_FIELD_LEN, validateJoin, type JoinErrors } from '../lib/validation';
  import type { JoinParams } from '../lib/types';
  import Icon from './Icon.svelte';

  const STORAGE_KEY = 'sheeshcord:entrada';

  let { store }: { store: RoomStore } = $props();

  function loadSaved(): JoinParams {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) return { serverUrl: '', room: '', name: '', ...JSON.parse(raw) };
    } catch {
      // Sem armazenamento: começa vazio.
    }
    return { serverUrl: '', room: '', name: '' };
  }

  let form = $state<JoinParams>(loadSaved());
  let errors = $state<JoinErrors>({});
  let submitted = $state(false);
  let fields = $state<Partial<Record<keyof JoinParams, HTMLInputElement>>>({});

  // Depois da primeira tentativa, revalida enquanto a pessoa corrige.
  $effect(() => {
    if (submitted) errors = validateJoin(form);
  });

  const busy = $derived(store.busy || store.status === 'connecting');

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    submitted = true;
    errors = validateJoin(form);
    if (hasErrors(errors)) {
      const first = (['serverUrl', 'room', 'name'] as const).find((k) => errors[k]);
      if (first) fields[first]?.focus();
      return;
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(form));
    } catch {
      // Ignora: salvar é só conveniência.
    }
    await store.join(form);
  }

  const inputs: { key: keyof JoinParams; label: string; placeholder: string; autocomplete: string; hint?: string }[] = [
    {
      key: 'serverUrl',
      label: 'Servidor',
      placeholder: 'ws://26.1.2.3:7880',
      autocomplete: 'url',
      hint: 'O IP de quem hospeda, na Radmin VPN.',
    },
    { key: 'room', label: 'Sala', placeholder: 'resenha', autocomplete: 'off' },
    { key: 'name', label: 'Seu nome', placeholder: 'Como a turma te chama', autocomplete: 'nickname' },
  ];
</script>

<main class="entrada">
  <section class="marca" aria-labelledby="titulo-app">
    <h1 id="titulo-app">Sheeshcord</h1>
    <p class="lema">Voz e tela para a sua turma, sem sair da Radmin.</p>
    <div class="vu" aria-hidden="true">
      {#each [0.35, 0.62, 0.48, 0.9, 0.55, 0.74, 0.3] as h, i (i)}
        <span style="--i: {i}; --h: {h}"></span>
      {/each}
    </div>
  </section>

  <form class="formulario" novalidate onsubmit={submit} aria-labelledby="titulo-form">
    <h2 id="titulo-form">Entrar numa sala</h2>

    {#each inputs as input (input.key)}
      {@const error = errors[input.key]}
      {@const describedBy = [error ? `${input.key}-erro` : '', input.hint ? `${input.key}-dica` : '']
        .filter(Boolean)
        .join(' ')}
      <div class="grupo">
        <label for="campo-{input.key}">{input.label}</label>
        <input
          bind:this={fields[input.key]}
          id="campo-{input.key}"
          class="campo"
          name={input.key}
          type="text"
          inputmode={input.key === 'serverUrl' ? 'url' : undefined}
          spellcheck="false"
          autocomplete={input.autocomplete as AutoFill}
          placeholder={input.placeholder}
          maxlength={input.key === 'serverUrl' ? undefined : MAX_FIELD_LEN + 16}
          bind:value={form[input.key]}
          aria-invalid={error ? 'true' : undefined}
          aria-describedby={describedBy || undefined}
          required
        />
        {#if input.hint}
          <p class="dica" id="{input.key}-dica">{input.hint}</p>
        {/if}
        {#if error}
          <p class="erro" id="{input.key}-erro">
            <Icon name="alert" size={16} />
            {error}
          </p>
        {/if}
      </div>
    {/each}

    {#if store.error}
      <div class="falha" role="alert">
        <Icon name="alert" size={18} />
        <span>Não deu para entrar: {store.error}</span>
      </div>
    {/if}

    <button class="botao botao-primario enviar" type="submit" disabled={busy} aria-busy={busy}>
      {busy ? 'Conectando…' : 'Entrar na sala'}
    </button>
  </form>
</main>

<style>
  .entrada {
    min-height: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(320px, 420px);
    align-items: center;
    gap: clamp(2rem, 6vw, 6rem);
    padding: clamp(1.5rem, 6vw, 5rem);
    box-sizing: border-box;
    overflow-y: auto;
  }

  h1 {
    margin: 0;
    font-size: clamp(3.5rem, 10vw, 8.5rem);
    line-height: 0.85;
    font-weight: 800;
    font-variation-settings: 'wdth' 75;
    letter-spacing: -0.035em;
  }

  .lema {
    margin: 1.25rem 0 0;
    max-width: 28ch;
    font-size: 1.25rem;
    color: var(--color-nevoa);
  }

  .vu {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    height: 72px;
    margin-top: 2.5rem;
  }

  .vu span {
    width: 14px;
    height: 100%;
    border-radius: 4px;
    background: var(--color-valvula);
    transform-origin: bottom;
    transform: scaleY(var(--h));
    animation: subir 1400ms var(--ease-saida) calc(var(--i) * 60ms) both;
  }

  .vu span:nth-child(3n + 2) {
    background: var(--color-giz);
  }

  /* Um único momento: o medidor "acende" ao abrir o app e assenta. */
  @keyframes subir {
    0% {
      transform: scaleY(0.04);
      opacity: 0;
    }
    40% {
      transform: scaleY(min(1, calc(var(--h) + 0.3)));
      opacity: 1;
    }
    100% {
      transform: scaleY(var(--h));
    }
  }

  .formulario {
    display: grid;
    gap: 1.1rem;
  }

  h2 {
    margin: 0 0 0.25rem;
    font-size: 1.5rem;
    font-weight: 700;
  }

  .grupo {
    display: grid;
    gap: 0.35rem;
  }

  label {
    font-weight: 600;
  }

  .dica {
    margin: 0;
    font-size: 0.875rem;
    color: var(--color-nevoa);
  }

  .erro {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0;
    font-size: 0.9rem;
    color: var(--color-corte);
  }

  .falha {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-botao);
    background: var(--color-corte-fundo);
    color: var(--color-giz);
  }

  .falha :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--color-corte);
  }

  .enviar {
    margin-top: 0.5rem;
    min-height: 52px;
    font-size: 1.05rem;
  }

  @media (max-width: 760px) {
    .entrada {
      grid-template-columns: 1fr;
      align-content: start;
    }

    .vu {
      display: none;
    }
  }
</style>
