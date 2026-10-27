<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { store, primaryAction, formatSize } from "../lib/store.svelte";
  import { runAction } from "../lib/actions";
  import ActionButton from "./ActionButton.svelte";

  let { id }: { id: string } = $props();

  const g = $derived(store.game(id));
  let confirming = $state(false);

  function uninstall() {
    confirming = false;
    store.act(id, (api) => api.uninstall(id), "remove");
  }
</script>

{#if g}
  {@const action = primaryAction(g, store.state!.needsAppUpdate)}
  {@const busy = store.busy(id)}
  {@const hasMods = g.mods !== "none"}
  <div class="detail">
    <div class="actions">
      <ActionButton {action} size="lg" progress={store.progress[id]} onclick={() => runAction(g, action)} />

      {#if g.game === "missing"}
        <button class="link" onclick={() => store.act(id, (api) => api.locate(id))}>Locate folder</button>
      {:else if confirming && hasMods && !busy}
        <div class="confirm" in:fly={{ x: -8, duration: 300 }}>
          <span>Remove all mods?</span>
          <button class="link danger" onclick={uninstall} disabled={g.running}>Remove</button>
          <button class="link" onclick={() => (confirming = false)}>Cancel</button>
        </div>
      {:else}
        {#if hasMods && !busy && g.game === "installed"}
          <button class="link" in:fade={{ duration: 200 }} onclick={() => (confirming = true)}>Uninstall mods</button>
        {/if}
        {#if g.path}
          <button class="link" onclick={() => store.api.openFolder(id)}>Open folder</button>
        {/if}
        <button class="link" onclick={() => (store.sheet = { report: id })}>Report a problem</button>
      {/if}
    </div>

    {#if store.errors[id]}
      <p class="error" transition:fade>{store.errors[id]}</p>
    {:else if g.running && hasMods && confirming}
      <p class="error" transition:fade>Close {g.name} first</p>
    {/if}

    <div class="grid">
      <section>
        <h3 class="label">{hasMods ? "Mods" : "Adds"}</h3>
        <ul class="mods">
          {#each g.latest.mods as m, i (m.name)}
            <li in:fly={{ y: 10, duration: 500, delay: 160 + i * 60, opacity: 0 }}>
              <div class="mod-head">
                <span class="mod-name">{m.name}</span>
                <span class="mono dim">{m.version}</span>
              </div>
              <p>{m.summary}</p>
            </li>
          {/each}
        </ul>
      </section>

      {#if g.latest.notes.length}
        <section>
          <h3 class="label">
            New in <span class="mono">{g.latest.version}</span>
          </h3>
          <ul class="notes">
            {#each g.latest.notes as n, i (n)}
              <li in:fly={{ y: 10, duration: 500, delay: 220 + i * 60, opacity: 0 }}>{n}</li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>

    <footer class="mono dim">{g.latest.version} · {formatSize(g.latest.size)} · {g.latest.released}</footer>
  </div>
{/if}

<style>
  .detail {
    padding-top: 8px;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 26px;
    min-height: 50px;
  }

  .link {
    font-size: 13.5px;
    color: var(--text-2);
    padding: 6px 0;
    border-bottom: 1px solid transparent;
    transition:
      color 0.2s,
      border-color 0.2s;
  }
  .link:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .danger {
    color: var(--error);
  }
  .danger:hover:not(:disabled) {
    color: #ffb1a6;
    border-color: rgba(255, 143, 128, 0.4);
  }
  .link:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .confirm {
    display: flex;
    align-items: center;
    gap: 18px;
    font-size: 13.5px;
    color: var(--text);
  }

  .error {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--error);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 56px;
    margin-top: 34px;
  }

  h3 {
    margin: 0 0 4px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }
  h3 .mono {
    font-size: 11px;
    letter-spacing: 0;
    color: var(--text-2);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .mods li {
    padding: 16px 0;
    border-bottom: 1px solid var(--line);
  }
  .mod-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
  }
  .mod-name {
    font-family: var(--serif);
    font-size: 28px;
    line-height: 1.05;
    letter-spacing: -0.01em;
  }
  .mods p {
    margin: 4px 0 0;
    color: var(--text-2);
  }

  .notes li {
    position: relative;
    padding: 14px 0 14px 20px;
    color: var(--text-2);
    border-bottom: 1px solid var(--line);
  }
  .notes li::before {
    content: "";
    position: absolute;
    left: 2px;
    top: 22px;
    width: 6px;
    height: 6px;
    background: var(--cyan);
    /* a single pixel from the sky */
    image-rendering: pixelated;
  }

  .dim {
    color: var(--text-3);
  }

  footer {
    margin-top: 28px;
    padding-top: 4px;
  }
</style>
