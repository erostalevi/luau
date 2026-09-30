<script lang="ts">
  // Render a short inline Markdown string safely (bold, italic, code, strike,
  // links as text, card links as titles, tags, mentions, dates) without HTML.
  import { resolveTitle } from '$lib/links/titles.svelte';
  import { fmtDate } from '$lib/i18n/index.svelte';

  let { text }: { text: string } = $props();

  type Seg = { k: 'text' | 'b' | 'i' | 'code' | 's' | 'link' | 'card' | 'tag' | 'mention' | 'date' | 'hl'; v: string };

  const RE = /(\*\*[^*]+\*\*|__[^_]+__|\*[^*\s][^*]*\*|_[^_\s][^_]*_|`[^`]+`|~~[^~]+~~|==[^=]+==|!?\[\[[^\]]+\]\]|\[[^\]]*\]\([^)]*\)|(?:^|(?<=\s))#[\p{L}\p{N}_/-]+|(?:^|(?<=\s))@[\p{L}\p{N}][\p{L}\p{N}_.-]*|\[\d{4}-\d{2}-\d{2}\])/gu;

  const segs = $derived.by((): Seg[] => {
    const out: Seg[] = [];
    let last = 0;
    for (const m of text.matchAll(RE)) {
      const s = m[0];
      if (m.index! > last) out.push({ k: 'text', v: text.slice(last, m.index) });
      if (s.startsWith('**') || s.startsWith('__')) out.push({ k: 'b', v: s.slice(2, -2) });
      else if (s.startsWith('`')) out.push({ k: 'code', v: s.slice(1, -1) });
      else if (s.startsWith('~~')) out.push({ k: 's', v: s.slice(2, -2) });
      else if (s.startsWith('==')) out.push({ k: 'hl', v: s.slice(2, -2) });
      else if (s.startsWith('[[') || s.startsWith('![[')) out.push({ k: 'card', v: s.replace(/^!?\[\[|\]\]$/g, '').split('|')[0].split('#')[0] });
      else if (s.startsWith('[') && s.includes('](')) out.push({ k: 'link', v: s.slice(1, s.indexOf('](')) });
      else if (s.startsWith('[')) out.push({ k: 'date', v: s.slice(1, -1) });
      else if (s.startsWith('#')) out.push({ k: 'tag', v: s });
      else if (s.startsWith('@')) out.push({ k: 'mention', v: s.slice(1) });
      else out.push({ k: 'i', v: s.slice(1, -1) });
      last = m.index! + s.length;
    }
    if (last < text.length) out.push({ k: 'text', v: text.slice(last) });
    return out;
  });
</script>

{#each segs as s, i (i)}{#if s.k === 'text'}{s.v}{:else if s.k === 'b'}<strong>{s.v}</strong>{:else if s.k === 'i'}<em>{s.v}</em>{:else if s.k === 'code'}<code>{s.v}</code>{:else if s.k === 's'}<s>{s.v}</s>{:else if s.k === 'hl'}<mark>{s.v}</mark>{:else if s.k === 'link'}<span class="lk">{s.v}</span>{:else if s.k === 'card'}<span class="card-link">{resolveTitle(s.v)}</span>{:else if s.k === 'tag'}<span class="tg">{s.v}</span>{:else if s.k === 'mention'}<span class="mn">@{s.v}</span>{:else if s.k === 'date'}<span class="dt">{fmtDate(s.v)}</span>{/if}{/each}

<style>
  code {
    font-family: var(--font-mono);
    font-size: 0.92em;
    padding: 0 4px;
    border-radius: 4px;
    background: var(--bg-hover);
  }
  .lk,
  .card-link {
    color: var(--primary-strong);
  }
  .card-link {
    font-weight: var(--fw-medium);
  }
  .tg {
    color: var(--ink-3);
  }
  .mn {
    color: var(--secondary-ink);
    font-weight: var(--fw-medium);
  }
  .dt {
    color: var(--primary-strong);
    font-weight: var(--fw-medium);
  }
</style>
