<script lang="ts">
  import { fade } from "svelte/transition";
  import { store } from "../lib/store.svelte";

  let { gameId }: { gameId: string | null } = $props();

  const g = $derived(gameId ? store.game(gameId) : undefined);
  let note = $state("");
  let phase = $state<"edit" | "sending" | "sent">("edit");
  let error = $state("");

  async function send() {
    phase = "sending";
    error = "";
    try {
      await store.api.report(gameId, note.trim());
      phase = "sent";
      setTimeout(() => (store.sheet = null), 1400);
    } catch (e) {
      error = String(e);
      phase = "edit";
    }
  }
</script>

<h2>Report a problem</h2>

{#if phase === "sent"}
  <p class="sent" in:fade>Sent. Thanks.</p>
{:else}
  <!-- svelte-ignore a11y_autofocus -->
  <textarea bind:value={note} placeholder="What happened?" autofocus disabled={phase === "sending"}></textarea>
  <p class="fine">Includes {g ? `${g.name} and ` : ""}app logs</p>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="spacer"></div>
  <button class="send" onclick={send} disabled={phase === "sending"}>
    {phase === "sending" ? "Sending" : "Send report"}
  </button>
{/if}

<style>
  h2 {
    margin: 0 0 24px;
    font-family: var(--serif);
    font-weight: 400;
    font-size: 44px;
    line-height: 1;
    letter-spacing: -0.015em;
  }

  textarea {
    width: 100%;
    height: 180px;
    resize: none;
    padding: 14px 16px;
    font: inherit;
    font-size: 14px;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    user-select: text;
    transition: border-color 0.2s;
  }
  textarea:focus {
    outline: none;
    border-color: rgba(143, 220, 240, 0.6);
  }
  textarea::placeholder {
    color: var(--text-3);
  }

  .fine {
    margin: 10px 2px 0;
    font-size: 12.5px;
    color: var(--text-3);
  }

  .error {
    margin: 10px 2px 0;
    font-size: 13px;
    color: var(--error);
  }

  .spacer {
    flex: 1;
  }

  .send {
    height: 46px;
    border-radius: 999px;
    font-size: 14px;
    font-weight: 560;
    color: var(--ink);
    background: var(--cream);
    transition:
      transform 0.25s var(--ease),
      opacity 0.2s;
  }
  .send:hover:not(:disabled) {
    transform: translateY(-1px);
  }
  .send:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .sent {
    font-family: var(--serif);
    font-style: italic;
    font-size: 28px;
    color: var(--cyan);
  }
</style>
