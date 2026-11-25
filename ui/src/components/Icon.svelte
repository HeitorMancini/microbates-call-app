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
    | 'alert'
    | 'roda'
    | 'settings'
    | 'phone-leave'
    | 'chat'
    | 'chevron-down';
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
  {:else if name === 'roda'}
    <circle cx="12" cy="5" r="2.2" />
    <circle cx="19" cy="12" r="2.2" />
    <circle cx="12" cy="19" r="2.2" />
    <circle cx="5" cy="12" r="2.2" />
  {:else if name === 'settings'}
    <path
      d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
    />
    <circle cx="12" cy="12" r="3" />
  {:else if name === 'phone-leave'}
    <path
      d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.12.96.36 1.9.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.91.34 1.85.58 2.81.7A2 2 0 0 1 22 16.92z"
    />
    <path d="M16 2h6v6M22 2l-7 7" />
  {:else if name === 'chat'}
    <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
  {:else if name === 'chevron-down'}
    <path d="m6 9 6 6 6-6" />
  {/if}
</svg>
