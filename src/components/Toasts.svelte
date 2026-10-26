<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { store } from "../lib/store.svelte";
</script>

<div class="toasts">
  {#each store.toasts as t (t.id)}
    <div class="toast {t.tone}" animate:flip={{ duration: 300 }} in:fly={{ y: 16, duration: 420 }} out:fly={{ y: 8, duration: 260 }}>
      {t.text}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: absolute;
    z-index: 40;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    padding: 10px 18px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--text);
    background: rgba(16, 24, 36, 0.92);
    border: 1px solid var(--line-strong);
    backdrop-filter: blur(10px);
    box-shadow: 0 12px 40px -12px rgba(0, 0, 0, 0.8);
    white-space: nowrap;
  }
  .toast.error {
    color: var(--error);
  }
</style>
