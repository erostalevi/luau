<script lang="ts">
  let { checked = $bindable(false), onchange, label, disabled = false }: { checked?: boolean; onchange?: (v: boolean) => void; label?: string; disabled?: boolean } = $props();
</script>

<button
  class="toggle"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 34px;
    height: 20px;
    flex-shrink: 0;
    border: none;
    border-radius: 999px;
    background: var(--line-strong);
    padding: 0;
    transition: background var(--dur) var(--ease);
  }
  .toggle.on {
    background: var(--primary);
  }
  .toggle:disabled {
    opacity: 0.5;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
    transition: transform var(--dur) var(--ease-spring);
  }
  .on .knob {
    transform: translateX(14px);
  }
</style>
