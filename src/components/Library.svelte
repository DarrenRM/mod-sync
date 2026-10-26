<script lang="ts">
  import { fly } from "svelte/transition";
  import { store, primaryAction, statusLine } from "../lib/store.svelte";
  import { runAction } from "../lib/actions";
  import ActionButton from "./ActionButton.svelte";

  const games = $derived(store.visibleGames);
</script>

<div class="library">
  <div class="head">
    <span class="label">Games</span>
  </div>

  <ul>
    {#each games as g, i (g.id)}
      {@const action = primaryAction(g, store.state!.needsAppUpdate)}
      {@const st = statusLine(g)}
      <li in:fly={{ y: 12, duration: 600, delay: 140 + i * 70, opacity: 0 }}>
        <div
          class="row"
          class:missing={g.game === "missing"}
          role="button"
          tabindex="0"
          onclick={() => (store.view = { name: "game", id: g.id })}
          onkeydown={(e) => e.key === "Enter" && (store.view = { name: "game", id: g.id })}
        >
          {#if store.art[g.id]?.hero}
            <div class="strip" style:background-image={`url(${store.art[g.id]?.hero})`}></div>
          {/if}
          <div class="name">
            <h2>{g.name}</h2>
            <p class="mods">{g.latest.mods.map((m) => m.name).join("  ·  ")}</p>
          </div>

          <span class="status mono {st.tone}">{st.text}</span>

          <ActionButton {action} progress={store.progress[g.id]} onclick={() => runAction(g, action)} />

          <svg class="chev" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
            <path d="m6 3.5 4.5 4.5L6 12.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </div>
        {#if store.errors[g.id]}
          <p class="error">{store.errors[g.id]}</p>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .library {
    padding-top: 10px;
  }

  .head {
    display: flex;
    justify-content: space-between;
    padding: 14px 0 12px;
    border-bottom: 1px solid var(--line);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    border-bottom: 1px solid var(--line);
  }

  .row {
    position: relative;
    overflow: hidden;
    display: grid;
    grid-template-columns: 1fr auto auto 16px;
    align-items: center;
    gap: 28px;
    padding: 22px 18px 22px 0;
    margin: 0 -18px 0 0;
    border-radius: 14px;
    outline: none;
    cursor: pointer;
    transition: background 0.25s var(--ease);
  }
  .row:hover,
  .row:focus-visible {
    background: linear-gradient(90deg, transparent, var(--bg-hover) 30%);
  }

  .row > :not(.strip) {
    position: relative;
  }

  .strip {
    position: absolute;
    inset: 0 0 0 30%;
    background-size: cover;
    background-position: center 40%;
    opacity: 0.16;
    mask: linear-gradient(90deg, transparent, #000 45%, #000 70%, transparent);
    transition:
      opacity 0.5s var(--ease),
      transform 0.8s var(--ease);
    pointer-events: none;
  }
  .row:hover .strip {
    opacity: 0.3;
    transform: scale(1.03);
  }
  .missing .strip {
    opacity: 0.07;
  }

  .name {
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-family: var(--serif);
    font-weight: 400;
    font-size: 46px;
    line-height: 1;
    letter-spacing: -0.015em;
    color: var(--text);
    transition: transform 0.4s var(--ease);
  }
  .row:hover h2 {
    transform: translateX(4px);
  }
  .missing h2 {
    color: var(--text-3);
  }

  .mods {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: transform 0.4s var(--ease) 0.03s;
  }
  .row:hover .mods {
    transform: translateX(4px);
  }
  .missing .mods {
    color: var(--text-3);
  }

  .status {
    color: var(--text-3);
    text-align: right;
  }
  .status.ok {
    color: var(--cyan);
  }
  .status.new {
    color: var(--cream);
  }
  .status.warn {
    color: var(--warn);
  }

  .chev {
    color: var(--text-3);
    transition:
      transform 0.3s var(--ease),
      color 0.3s;
  }
  .row:hover .chev {
    color: var(--text);
    transform: translateX(3px);
  }

  .error {
    margin: -8px 0 18px;
    font-size: 13px;
    color: var(--error);
  }
</style>
