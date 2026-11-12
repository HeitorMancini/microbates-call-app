<script lang="ts" module>
  export type IconName =
    | 'mic'
    | 'mic-off'
    | 'headphones'
    | 'headphones-off'
    | 'screen'
    | 'screen-off'
    | 'sliders'
    | 'leave'
    | 'expand'
    | 'shrink'
    | 'close'
    | 'alert';
</script>

<script lang="ts">
  let { name, size = 22 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === 'mic' || name === 'mic-off'}
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
    {#if name === 'mic-off'}<path d="M3 3l18 18" />{/if}
  {:else if name === 'headphones' || name === 'headphones-off'}
    <path d="M4 15v-3a8 8 0 0 1 16 0v3" />
    <rect x="3" y="14" width="5" height="7" rx="2" />
    <rect x="16" y="14" width="5" height="7" rx="2" />
    {#if name === 'headphones-off'}<path d="M3 3l18 18" />{/if}
  {:else if name === 'screen' || name === 'screen-off'}
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
    {#if name === 'screen'}<path d="M12 13V7M9 9.5 12 7l3 2.5" />{:else}<path d="M9.5 7.5l5 5M14.5 7.5l-5 5" />{/if}
  {:else if name === 'sliders'}
    <path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12" />
    <circle cx="16" cy="6" r="2" />
    <circle cx="10" cy="12" r="2" />
    <circle cx="18" cy="18" r="2" />
  {:else if name === 'leave'}
    <path d="M14 4h4a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4" />
    <path d="M10 16l-4-4 4-4M6 12h10" />
  {:else if name === 'expand'}
    <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
  {:else if name === 'shrink'}
    <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />
  {:else if name === 'close'}
    <path d="M6 6l12 12M18 6 6 18" />
  {:else if name === 'alert'}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v6M12 16.5v.5" />
  {/if}
</svg>
