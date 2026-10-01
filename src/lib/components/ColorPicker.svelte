<script lang="ts">
  import { hexToRgb, rgbToHex } from '$lib/theme/color';

  let { value = $bindable('#ef8a7c'), onchange, swatches = DEFAULT_SWATCHES }: { value?: string; onchange?: (hex: string) => void; swatches?: string[] } = $props();

  // HSV model for the picker surface.
  function toHsv(hex: string): [number, number, number] {
    const [r, g, b] = hexToRgb(hex).map((x) => x / 255);
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const d = max - min;
    let h = 0;
    if (d) {
      if (max === r) h = ((g - b) / d) % 6;
      else if (max === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
      h *= 60;
      if (h < 0) h += 360;
    }
    return [h, max ? d / max : 0, max];
  }
  function fromHsv(h: number, s: number, v: number): string {
    const c = v * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = v - c;
    const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
    return rgbToHex([(r + m) * 255, (g + m) * 255, (b + m) * 255]);
  }

  let hsv = $state(toHsv(value));
  let hexInput = $state(value);

  function commit(hex: string, fromHsvChange = false) {
    value = hex;
    hexInput = hex;
    if (!fromHsvChange) hsv = toHsv(hex);
    onchange?.(hex);
  }

  function drag(e: PointerEvent, kind: 'sv' | 'h') {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const update = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width));
      const y = Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height));
      if (kind === 'sv') hsv = [hsv[0], x, 1 - y];
      else hsv = [x * 360, hsv[1], hsv[2]];
      commit(fromHsv(hsv[0], hsv[1], hsv[2]), true);
    };
    update(e);
    const up = () => {
      el.removeEventListener('pointermove', update);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', update);
    el.addEventListener('pointerup', up);
  }
</script>

<script lang="ts" module>
  export const DEFAULT_SWATCHES = [
    // Hawaii sunset first (coral, hibiscus, mango, plumeria, dusk), then the rest.
    '#ef8a7c', '#ec8fb0', '#f5a97f', '#f3c77e', '#b99be0', '#8f9df0', '#7fc8c4', '#9fd49a',
    '#7a7cf0', '#c48cf0', '#86b8f0', '#9aa6c4', '#fdeadc', '#fde6ee', '#eef7ef', '#f4eff2',
  ];
</script>

<div class="cp">
  <div class="swatches">
    {#each swatches as s (s)}
      <button class="sw" class:on={s.toLowerCase() === value.toLowerCase()} style:background={s} aria-label={s} onclick={() => commit(s)}></button>
    {/each}
  </div>
  <div class="sv" role="slider" aria-valuenow={Math.round(hsv[1] * 100)} tabindex="0" style:background-color={fromHsv(hsv[0], 1, 1)} onpointerdown={(e) => drag(e, 'sv')}>
    <div class="knob" style:left="{hsv[1] * 100}%" style:top="{(1 - hsv[2]) * 100}%" style:background={value}></div>
  </div>
  <div class="hue" role="slider" aria-valuenow={Math.round(hsv[0])} tabindex="0" onpointerdown={(e) => drag(e, 'h')}>
    <div class="hknob" style:left="{(hsv[0] / 360) * 100}%"></div>
  </div>
  <div class="row">
    <span class="preview" style:background={value}></span>
    <input
      class="field mono"
      bind:value={hexInput}
      maxlength="7"
      spellcheck="false"
      oninput={() => {
        if (/^#[0-9a-f]{6}$/i.test(hexInput)) commit(hexInput.toLowerCase());
      }}
    />
  </div>
</div>

<style>
  .cp {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 260px;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 6px;
  }
  .sw {
    aspect-ratio: 1;
    border-radius: 8px;
    border: none;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .sw:hover {
    transform: scale(1.1);
  }
  .sw.on {
    box-shadow:
      0 0 0 2px var(--bg-elev),
      0 0 0 4px var(--primary);
  }
  .sv {
    position: relative;
    height: 150px;
    border-radius: 10px;
    background-image: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent);
    cursor: crosshair;
    touch-action: none;
  }
  .knob,
  .hknob {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid #fff;
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }
  .hue {
    position: relative;
    height: 12px;
    border-radius: 999px;
    background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
    touch-action: none;
  }
  .hknob {
    top: 50%;
    background: transparent;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .preview {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    flex-shrink: 0;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
  }
</style>
