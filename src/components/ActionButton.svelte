<script lang="ts">
  import type { Action } from "../lib/store.svelte";
  import { progressLabel, syncPercent } from "../lib/store.svelte";
  import type { Progress } from "../lib/types";

  let {
    action,
    progress,
    size = "md",
    onclick,
  }: { action: Action; progress?: Progress; size?: "md" | "lg"; onclick: () => void } = $props();

  const pct = $derived(progress ? syncPercent(progress) : null);
  const label = $derived(progress ? progressLabel(progress) : action.label);
  const quiet = $derived(action.kind === "none" || action.kind === "steam");
</script>

<!-- While busy: a dark track with cream text, and a cream fill carrying dark text, revealed from the
     left by clip-path. The two labels line up exactly, so the text changes colour as the fill passes. -->
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
  {#if progress && pct === null}
    <span class="shimmer"></span>
  {/if}
  <span class="text">{label}</span>
  {#if progress && pct !== null}
    <span class="fill" style:clip-path={`inset(0 ${(100 - pct * 100).toFixed(2)}% 0 0)`} aria-hidden="true">
      <span class="text">{label}</span>
    </span>
  {/if}
</button>

<style>
  .btn {
    position: relative;
    overflow: hidden;
    flex: none;
    display: inline-grid;
    place-items: center;
    min-width: 140px;
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

  /* Empty track: same pill, dark inside, a thin cream edge. */
  .busy {
    color: var(--cream);
    background: rgba(246, 241, 217, 0.05);
    box-shadow: 0 0 0 1px rgba(246, 241, 217, 0.3) inset;
  }

  .fill {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--ink);
    background: var(--cream);
    /* Glides between progress updates instead of stepping. */
    transition: clip-path 0.6s cubic-bezier(0.22, 1, 0.36, 1);
  }

  /* No measurable progress (removing mods): a soft sweep behind the label. */
  .shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, rgba(246, 241, 217, 0.16), transparent);
    background-size: 50% 100%;
    background-repeat: no-repeat;
    animation: sweep 1.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }
  @keyframes sweep {
    from {
      background-position: -100% 0;
    }
    to {
      background-position: 200% 0;
    }
  }

  .text {
    position: relative;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
