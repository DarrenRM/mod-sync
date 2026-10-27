<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { store } from "./lib/store.svelte";
  import { checkAppUpdate, installAppUpdate } from "./lib/updater";
  import TitleBar from "./components/TitleBar.svelte";
  import Hero from "./components/Hero.svelte";
  import Library from "./components/Library.svelte";
  import GameDetail from "./components/GameDetail.svelte";
  import Sheet from "./components/Sheet.svelte";
  import Settings from "./components/Settings.svelte";
  import Report from "./components/Report.svelte";
  import Toasts from "./components/Toasts.svelte";

  onMount(() => {
    store.init().then(() => checkAppUpdate());
    // Keep "running" and Steam state fresh while the window is open; check for app updates every 6 h.
    // While offline, also retry the server every 30 s so the app recovers by itself.
    let ticks = 0;
    const poll = setInterval(() => {
      ticks++;
      if (document.visibilityState === "visible") store.refresh(!!store.state?.offline && ticks % 6 === 0);
    }, 5000);
    const updates = setInterval(async () => {
      if ((await checkAppUpdate()) === "ready" && document.visibilityState === "hidden") installAppUpdate();
    }, 6 * 3600 * 1000);
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (store.sheet) store.closeSheet();
      else if (store.view.name === "game") store.view = { name: "library" };
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      clearInterval(poll);
      clearInterval(updates);
    };
  });

  const viewKey = $derived(store.view.name === "game" ? `game:${store.view.id}` : "library");
</script>

<div class="shell">
  <Hero />
  <TitleBar />

  <section class="content">
    {#if store.state}
      {#key viewKey}
        <div class="page" in:fly={{ y: 14, duration: 520, delay: 90, opacity: 0 }} out:fade={{ duration: 90 }}>
          {#if store.view.name === "game"}
            <GameDetail id={store.view.id} />
          {:else}
            <Library />
          {/if}
        </div>
      {/key}
    {/if}
  </section>

  <Sheet open={store.sheet !== null} onclose={() => store.closeSheet()}>
    {#if store.sheet === "settings"}
      <Settings />
    {:else if store.sheet}
      <Report gameId={store.sheet.report} />
    {/if}
  </Sheet>

  <Toasts />
</div>

<style>
  .shell {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .content {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .page {
    position: absolute;
    inset: 0;
    padding: 0 var(--gutter) 40px;
  }
</style>
