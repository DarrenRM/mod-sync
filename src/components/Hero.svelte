<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import sky from "../assets/sky.webp";
  import { store, heroStatus } from "../lib/store.svelte";

  const game = $derived(store.view.name === "game" ? store.game(store.view.id) : undefined);
  const art = $derived(game ? store.art[game.id] : undefined);

  const summary = $derived.by(() => {
    const games = store.state?.games ?? [];
    const updates = games.filter((g) => g.game === "installed" && (g.mods === "outdated" || g.mods === "broken")).length;
    if (!store.state) return "";
    if (store.state.needsAppUpdate) return "App update required";
    if (updates) return updates === 1 ? "1 update" : `${updates} updates`;
    if (games.some((g) => g.mods === "current")) return "Up to date";
    return `${games.length} ${games.length === 1 ? "game" : "games"}`;
  });

  const tone = $derived(
    store.state?.needsAppUpdate || summary.includes("update") ? "new" : summary === "Up to date" ? "ok" : "dim",
  );
</script>

<header class="hero" class:compact={!!game}>
  <div class="sky" class:hidden={!!art?.hero} style:background-image={`url(${sky})`}></div>
  {#key art?.hero}
    {#if art?.hero}
      <div class="art" style:background-image={`url(${art.hero})`} transition:fade={{ duration: 700 }}></div>
    {/if}
  {/key}
  <div class="shade" class:on-art={!!art?.hero}></div>

  {#key game?.id ?? "home"}
    <div class="title" in:fly={{ y: 18, duration: 640, delay: 60, opacity: 0 }} out:fade={{ duration: 120 }}>
      {#if game}
        <button class="back" onclick={() => (store.view = { name: "library" })}>
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M8.5 3 4.5 7l4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
          Library
        </button>
        {#if art?.logo}
          <h1 class="logo-wrap"><img class="logo" src={art.logo} alt={game.name} /></h1>
        {:else}
          <h1>{game.name}</h1>
        {/if}
        {@const st = heroStatus(game)}
        <p class="status {st.tone}"><span class="dot"></span>{st.text}</p>
      {:else}
        <h1>Mod <em>Sync</em></h1>
        <p class="status {tone}"><span class="dot"></span>{summary}</p>
      {/if}
    </div>
  {/key}
</header>

<style>
  .hero {
    position: relative;
    flex: none;
    height: 318px;
    overflow: hidden;
    transition: height 0.6s var(--ease);
  }
  .hero.compact {
    height: 248px;
  }
  .compact h1 {
    font-size: 88px;
  }

  .sky {
    position: absolute;
    inset: -6% -8%;
    background-size: cover;
    background-position: 30% 38%;
    animation: drift 70s ease-in-out infinite alternate;
    will-change: transform;
  }

  .sky {
    transition: opacity 0.7s var(--ease);
  }
  .sky.hidden {
    opacity: 0;
  }

  .art {
    position: absolute;
    inset: -4%;
    background-size: cover;
    background-position: 60% 35%;
    animation: drift 90s ease-in-out infinite alternate;
  }

  .shade.on-art {
    background:
      linear-gradient(180deg, rgba(5, 8, 14, 0.35) 0%, rgba(5, 8, 14, 0.1) 30%, rgba(5, 8, 14, 0.7) 72%, var(--bg) 97%),
      linear-gradient(90deg, rgba(5, 8, 14, 0.7) 0%, rgba(5, 8, 14, 0) 60%);
  }

  .logo-wrap {
    line-height: 0;
  }
  .logo {
    display: block;
    height: 84px;
    max-width: 60%;
    object-fit: contain;
    object-position: left bottom;
    filter: drop-shadow(0 4px 24px rgba(0, 0, 0, 0.5));
  }

  @keyframes drift {
    from {
      transform: translate3d(0, 0, 0) scale(1.02);
    }
    to {
      transform: translate3d(-5%, -2%, 0) scale(1.08);
    }
  }

  .shade {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(180deg, rgba(5, 8, 14, 0.15) 0%, rgba(5, 8, 14, 0) 28%, rgba(5, 8, 14, 0.6) 68%, var(--bg) 97%),
      linear-gradient(90deg, rgba(5, 8, 14, 0.45) 0%, rgba(5, 8, 14, 0) 55%);
  }

  .hero::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 12px;
    background: var(--bg);
    mask: linear-gradient(transparent, #000);
  }

  .title {
    position: absolute;
    left: var(--gutter);
    right: var(--gutter);
    bottom: 26px;
  }

  h1 {
    margin: 0;
    font-family: var(--serif);
    font-weight: 400;
    font-size: 104px;
    line-height: 0.92;
    letter-spacing: -0.025em;
    color: var(--cream);
    text-shadow: 0 2px 30px rgba(3, 20, 45, 0.35);
  }

  h1 em {
    font-style: italic;
    color: rgba(246, 241, 217, 0.62);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 0;
    font-size: 11px;
    font-weight: 560;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-2);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .ok .dot {
    background: var(--cyan);
    box-shadow: 0 0 12px rgba(143, 220, 240, 0.7);
  }
  .new .dot {
    background: var(--cream);
    box-shadow: 0 0 12px rgba(246, 241, 217, 0.7);
    animation: pulse 2.4s ease-in-out infinite;
  }
  .warn .dot {
    background: var(--warn);
  }
  .new {
    color: var(--cream);
  }

  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }

  .back {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 14px -2px;
    padding: 6px 10px 6px 6px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-2);
    background: rgba(5, 8, 14, 0.28);
    backdrop-filter: blur(8px);
    transition:
      color 0.2s,
      background 0.2s;
  }
  .back:hover {
    color: var(--text);
    background: rgba(5, 8, 14, 0.5);
  }
</style>
