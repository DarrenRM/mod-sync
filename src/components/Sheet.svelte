<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";

  let { open, onclose, children }: { open: boolean; onclose: () => void; children: Snippet } = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" transition:fade={{ duration: 260 }} onclick={onclose}></div>
  <aside class="sheet" transition:fly={{ x: 420, duration: 480, easing: cubicOut, opacity: 1 }}>
    <button class="x" aria-label="Close" onclick={onclose}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 4.5 7 7m0-7-7 7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
    </button>
    {@render children()}
  </aside>
{/if}

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 30;
    background: rgba(3, 5, 9, 0.55);
    backdrop-filter: blur(3px);
  }

  .sheet {
    position: absolute;
    z-index: 31;
    top: 0;
    right: 0;
    bottom: 0;
    width: 400px;
    padding: 64px 40px 32px;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border-left: 1px solid var(--line);
    box-shadow: -40px 0 80px -30px rgba(0, 0, 0, 0.8);
  }

  .x {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 34px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    color: var(--text-2);
    transition: background 0.18s;
  }
  .x:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
</style>
