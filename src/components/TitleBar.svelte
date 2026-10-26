<script lang="ts">
  import { fade } from "svelte/transition";
  import { store } from "../lib/store.svelte";
  import { installAppUpdate } from "../lib/updater";

  function drag(e: MouseEvent) {
    if (e.button === 0 && (e.target as HTMLElement).closest("button") === null) store.api?.window.drag();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="bar" onmousedown={drag}>
  <div class="left">
    {#if store.state?.offline}
      <span class="tag" transition:fade>Offline</span>
    {/if}
  </div>

  <div class="right">
    {#if store.appUpdate?.ready}
      <button class="pill" transition:fade onclick={installAppUpdate}>
        <span class="spark"></span>Restart to update
      </button>
    {/if}
    <button class="icon" aria-label="Settings" onclick={() => (store.sheet = "settings")}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
        <g fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
          <path d="M2.5 5h7M12.5 5h1M2.5 11h1M6.5 11h7" />
          <circle cx="11" cy="5" r="1.6" /><circle cx="5" cy="11" r="1.6" />
        </g>
      </svg>
    </button>
    <button class="icon" aria-label="Minimize" onclick={() => store.api?.window.minimize()}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 8.5h8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
    </button>
    <button class="icon close" aria-label="Close" onclick={() => store.api?.window.close()}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 4.5 7 7m0-7-7 7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
    </button>
  </div>
</div>

<style>
  .bar {
    position: absolute;
    z-index: 20;
    top: 0;
    left: 0;
    right: 0;
    height: 46px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 0 var(--gutter);
  }

  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .icon {
    width: 34px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    color: rgba(246, 241, 217, 0.78);
    transition:
      background 0.18s,
      color 0.18s;
  }
  .icon:hover {
    background: rgba(5, 8, 14, 0.35);
    color: var(--cream);
  }
  .close:hover {
    background: rgba(220, 70, 60, 0.85);
  }

  .tag {
    font-size: 11px;
    font-weight: 560;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    padding: 4px 9px;
    border-radius: 999px;
    color: var(--cream);
    background: rgba(5, 8, 14, 0.4);
    backdrop-filter: blur(8px);
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    margin-right: 6px;
    padding: 0 12px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 540;
    color: var(--ink);
    background: var(--cream);
    transition: transform 0.2s var(--ease);
  }
  .pill:hover {
    transform: translateY(-1px);
  }
  .spark {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--sky);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
</style>
