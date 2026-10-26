<script lang="ts">
  let { label, checked, onchange }: { label: string; checked: boolean; onchange: (v: boolean) => void } = $props();
</script>

<button class="row" role="switch" aria-checked={checked} onclick={() => onchange(!checked)}>
  <span>{label}</span>
  <span class="track" class:on={checked}><span class="knob"></span></span>
</button>

<style>
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 0;
    font-size: 14.5px;
    text-align: left;
    border-bottom: 1px solid var(--line);
  }

  .track {
    position: relative;
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: rgba(240, 236, 220, 0.12);
    transition: background 0.3s var(--ease);
  }
  .track.on {
    background: var(--cyan);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition:
      transform 0.35s var(--ease),
      background 0.3s;
  }
  .on .knob {
    transform: translateX(16px);
    background: var(--ink);
  }
</style>
