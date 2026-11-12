<script lang="ts">
  import type { Participant } from '../lib/types';
  import ParticipantTile from './ParticipantTile.svelte';

  let { participants, compact = false }: { participants: Participant[]; compact?: boolean } = $props();
</script>

{#if participants.length === 0}
  <p class="vazio">Ninguém por aqui ainda. Chame a turma para a sala.</p>
{:else}
  <ul class="lista" class:compact aria-label="Pessoas na sala ({participants.length})">
    {#each participants as participant (participant.id)}
      <ParticipantTile {participant} {compact} />
    {/each}
  </ul>
{/if}

<style>
  .lista {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 2.5rem 1.5rem;
    width: min(100%, 860px);
    margin: 0 auto;
    padding: 0;
  }

  .lista.compact {
    grid-template-columns: 1fr;
    gap: 1rem;
    width: 100%;
  }

  .vazio {
    color: var(--color-nevoa);
    text-align: center;
  }
</style>
