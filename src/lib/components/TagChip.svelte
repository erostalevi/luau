<script lang="ts">
  import { tagHue } from '$lib/markdown/meta';
  import { pastel, tintFromHex } from '$lib/theme/color';
  import { theme } from '$lib/theme/theme.svelte';
  import { settings } from '$lib/settings/store.svelte';

  let {
    tag,
    color = null,
    outline = false,
    prefix = '#',
    onclick,
  }: { tag: string; color?: string | null; outline?: boolean; prefix?: string; onclick?: (e: MouseEvent) => void } = $props();

  const custom = $derived(color ?? settings.get<Record<string, string>>('tags.colors')?.[tag.toLowerCase()] ?? null);
  const c = $derived(custom ? tintFromHex(custom, theme.dark) : pastel(tagHue(tag), theme.dark));
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_static_element_interactions -->
<span
  class="chip tag"
  class:outline
  class:clickable={!!onclick}
  style:background={outline ? 'transparent' : c.bg}
  style:color={c.ink}
  style:box-shadow={outline ? `inset 0 0 0 1px ${c.dot}` : undefined}
  role={onclick ? 'button' : undefined}
  tabindex={onclick ? -1 : undefined}
  {onclick}
  onkeydown={() => {}}
  title={prefix + tag}
>
  {#if prefix}<span class="p">{prefix}</span>{/if}{tag}
</span>

<style>
  .tag {
    gap: 0;
    font-weight: var(--fw-medium);
  }
  .p {
    opacity: 0.55;
    margin-right: 1px;
  }
  .clickable:hover {
    filter: brightness(0.97) saturate(1.1);
  }
</style>
