<script lang="ts">
  import type { Action } from "../lib/store.svelte";
  import { progressLabel } from "../lib/store.svelte";
  import type { Progress } from "../lib/types";

  let {
    action,
    progress,
    size = "md",
    onclick,
  }: { action: Action; progress?: Progress; size?: "md" | "lg"; onclick: () => void } = $props();

  const pct = $derived(progress && progress.total ? progress.done / progress.total : null);
  const quiet = $derived(action.kind === "none" || action.kind === "steam");
</script>

<button
  class="btn {size}"
  class:quiet
  class:busy={!!progress}
  disabled={action.disabled || !!progress}
  onclick={(e) => {
    e.stopPropagation();
    onclick();
  }}
>
  {#if progress}
    <span class="fill" class:indeterminate={pct === null} style:transform={pct === null ? undefined : `scaleX(${pct})`}></span>
  {/if}
  <span class="text">{progress ? progressLabel(progress) : action.label}</span>
</button>

<style>
  .btn {
    position: relative;
    overflow: hidden;
    flex: none;
    display: inline-grid;
    place-items: center;
    min-width: 128px;
    height: 40px;
    padding: 0 20px;
    border-radius: 999px;
    font-size: 13.5px;
    font-weight: 560;
    letter-spacing: 0.005em;
    color: var(--ink);
    background: var(--cream);
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.5) inset,
      0 8px 24px -10px rgba(246, 241, 217, 0.45);
    transition:
      transform 0.25s var(--ease),
      box-shadow 0.25s var(--ease),
      background 0.25s,
      color 0.25s;
  }
  .btn:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.5) inset,
      0 12px 30px -10px rgba(246, 241, 217, 0.6);
  }
  .btn:active:not(:disabled) {
    transform: translateY(0) scale(0.98);
  }

  .lg {
    min-width: 188px;
    height: 50px;
    padding: 0 30px;
    font-size: 15px;
  }

  .quiet {
    color: var(--text);
    background: rgba(240, 236, 220, 0.08);
    box-shadow: 0 0 0 1px var(--line-strong) inset;
  }
  .quiet:hover:not(:disabled) {
    background: rgba(240, 236, 220, 0.14);
    box-shadow: 0 0 0 1px rgba(240, 236, 220, 0.24) inset;
  }

  .btn:disabled {
    cursor: default;
  }
  .btn:disabled:not(.busy) {
    color: var(--text-3);
    background: transparent;
    box-shadow: 0 0 0 1px var(--line) inset;
  }

  .busy {
    color: var(--ink);
    background: rgba(246, 241, 217, 0.22);
    box-shadow: none;
  }

  .fill {
    position: absolute;
    inset: 0;
    background: var(--cream);
    transform-origin: left center;
    transition: transform 0.35s var(--ease);
  }
  .fill.indeterminate {
    background: linear-gradient(90deg, rgba(246, 241, 217, 0.15), var(--cream), rgba(246, 241, 217, 0.15));
    background-size: 200% 100%;
    animation: sweep 1.4s linear infinite;
  }
  @keyframes sweep {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }

  .text {
    position: relative;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .busy .text {
    mix-blend-mode: normal;
  }
</style>
