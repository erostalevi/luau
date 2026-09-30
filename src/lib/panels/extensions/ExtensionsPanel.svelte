<script lang="ts">
  import { Search, Blocks } from '@lucide/svelte';
  import { contributions } from '$lib/contributions/registry.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let q = $state('');
  const list = $derived(contributions.extensions.filter((e) => !q || `${e.name} ${e.description ?? ''}`.toLowerCase().includes(q.toLowerCase())));
</script>

<div class="ext">
  <label class="search">
    <Search size={14} class="muted" />
    <input bind:value={q} placeholder={t('extensions.search')} spellcheck="false" />
  </label>
  {#each list as e (e.id)}
    <div class="row">
      <Blocks size={15} />
      <span class="grow">{e.name}</span>
      <span class="muted small">{e.version}</span>
    </div>
  {:else}
    <div class="empty">
      <div class="empty-icon"><Blocks size={20} /></div>
      <strong>{t('extensions.emptyTitle')}</strong>
      <span class="small">{t('extensions.emptyHint')}</span>
    </div>
  {/each}
</div>

<style>
  .ext {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 10px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    box-shadow: inset 0 0 0 1px var(--line);
    margin-bottom: 6px;
  }
  input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
  }
  .small {
    font-size: var(--fs-xs);
    line-height: 1.5;
  }
</style>
