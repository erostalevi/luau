<script lang="ts">
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { setSetting } from '../effects';

  let { def, label }: { def: SettingDef; label: string } = $props();
  const value = $derived(String(settings.get(def.key) ?? ''));
  let draft = $state<string | null>(null);

  function commit() {
    if (draft === null) return;
    const v = draft.trim();
    draft = null;
    if (v !== value) setSetting(def.key, v);
  }
</script>

<input
  class="field"
  value={draft ?? value}
  placeholder={String(def.default ?? '')}
  aria-label={label}
  spellcheck="false"
  oninput={(e) => (draft = e.currentTarget.value)}
  onblur={commit}
  onkeydown={(e) => {
    if (e.key === 'Enter') commit();
    if (e.key === 'Escape') {
      draft = null;
      e.currentTarget.blur();
    }
  }}
/>

<style>
  input {
    max-width: 380px;
  }
</style>
