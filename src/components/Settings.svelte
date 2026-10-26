<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { checkAppUpdate, installAppUpdate } from "../lib/updater";
  import type { Settings } from "../lib/types";
  import Toggle from "./Toggle.svelte";

  let checking = $state(false);
  let result = $state<"" | "none" | "error">("");

  async function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    if (!store.state) return;
    const next = { ...store.state.settings, [key]: value };
    store.state.settings = next;
    try {
      store.state = await store.api.setSettings(next);
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  async function check() {
    checking = true;
    const r = await checkAppUpdate(true);
    checking = false;
    result = r === "ready" ? "" : r;
  }
</script>

{#if store.state}
  {@const s = store.state.settings}
  <h2>Settings</h2>

  <div class="toggles">
    <Toggle label="Keep mods updated" checked={s.autoUpdate} onchange={(v) => set("autoUpdate", v)} />
    <Toggle label="Start with Windows" checked={s.background} onchange={(v) => set("background", v)} />
    <Toggle label="Send crash logs" checked={s.shareLogs} onchange={(v) => set("shareLogs", v)} />
  </div>

  <div class="spacer"></div>

  {#if store.welcome}
    <button class="done" onclick={() => store.closeSheet()}>Done</button>
  {:else}
  <div class="about">
    <div>
      <p class="label">Version</p>
      <p class="mono">{store.state.appVersion}</p>
    </div>
    {#if store.appUpdate?.ready}
      <button class="btn" onclick={installAppUpdate}>Restart to {store.appUpdate.version}</button>
    {:else}
      <button class="btn" onclick={check} disabled={checking}>
        {checking ? "Checking" : result === "none" ? "Up to date" : "Check for updates"}
      </button>
    {/if}
  </div>
  {/if}
{/if}

<style>
  h2 {
    margin: 0 0 18px;
    font-family: var(--serif);
    font-weight: 400;
    font-size: 44px;
    line-height: 1;
    letter-spacing: -0.015em;
  }

  .toggles {
    border-top: 1px solid var(--line);
  }

  .spacer {
    flex: 1;
  }

  .about {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
  }
  .about p {
    margin: 0;
  }
  .about .mono {
    margin-top: 6px;
    color: var(--text-2);
  }

  .done {
    height: 50px;
    border-radius: 999px;
    background: var(--cream);
    color: var(--ink);
    font-size: 15px;
    font-weight: 600;
    transition: filter 0.2s;
  }
  .done:hover {
    filter: brightness(1.05);
  }

  .btn {
    height: 34px;
    padding: 0 16px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 540;
    box-shadow: 0 0 0 1px var(--line-strong) inset;
    transition: background 0.2s;
  }
  .btn:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .btn:disabled {
    color: var(--text-2);
    cursor: default;
  }
</style>
