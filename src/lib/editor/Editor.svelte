<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import type { EditorView as View } from '@codemirror/view';
  import { rpc, isTauri } from '$lib/backend/rpc';
  import type { Attachment, SearchHit } from '$lib/backend/types';
  import { boards } from '$lib/state/boards.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { ctx as cmdCtx } from '$lib/commands/context.svelte';
  import { resolveTitle, lookupCard } from '$lib/links/titles.svelte';
  import { openCardById } from '$lib/app/open';
  import { openExternal, reveal, pickFile } from '$lib/app/helpers';
  import { cardFileUrl, attachmentDirRel } from '$lib/board/paths';
  import { tagHue } from '$lib/markdown/meta';
  import { pastel } from '$lib/theme/color';
  import { theme } from '$lib/theme/theme.svelte';
  import { t, fmtDate, i18n } from '$lib/i18n/index.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { setActiveEditor, clearActiveEditor } from './active';
  import type { LuauEditorContext, CodeResult } from './cm/context';
  import { refreshPreview } from './cm/livePreview';
  import { fileMarkdown } from './cm/paste';
  import { renderMarkdown, enhanceRendered } from '$lib/markdown/render';
  import { runCodeCell } from '$lib/summaries/code';

  let {
    boardId,
    cardId,
    autofocus = false,
    readOnly = false,
    fullWidth = false,
    onconflict,
  }: { boardId: string; cardId: string; autofocus?: boolean; readOnly?: boolean; fullWidth?: boolean; onconflict?: (c: { mine: string; theirs: string } | null) => void } = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: View | null = null;
  let setup: typeof import('./cm/setup') | null = null;
  let loadedFor = '';
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let dirty = false;
  let lastSaved = '';
  /** The card the current `view` holds. Saves always target this, never the
   *  live props: those already point at the next card while switching. */
  type Target = { boardId: string; cardId: string };
  let target: Target | null = null;
  const targets = new WeakMap<View, Target>();
  const targetOf = (v: View | null) => (v ? (targets.get(v) ?? null) : null);
  let session = Math.random().toString(36).slice(2, 10);
  const outputs = new Map<string, CodeResult>();
  const previews = new Map<string, Promise<{ title: string; description: string; image: string | null; site: string } | null>>();

  const model = $derived(boards.get(boardId));
  const node = $derived(model?.nodes.get(cardId));

  /** The first line is always a `# Title` (restored when removed). */
  function ensureTitle(content: string): string {
    const first = content.split('\n', 1)[0];
    if (/^#(\s|$)/.test(first)) return content;
    if (!content.trim()) return `# ${t('common.untitled')}\n`;
    if (!first.trim()) return `# ${t('common.untitled')}\n${content}`;
    if (/^#{2,6}\s/.test(first)) return `# ${content.replace(/^#+\s*/, '')}`;
    return `# ${content}`;
  }

  async function save(now = false, v: View | null = view, tg: Target | null = targetOf(v)) {
    if (!v || !tg || readOnly) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
    const content = v.state.doc.toString();
    if (content === lastSaved) {
      dirty = false;
      return;
    }
    const doSave = async () => {
      const toWrite = ensureTitle(content);
      lastSaved = content;
      try {
        await rpc('card.write', { board: tg.boardId, id: tg.cardId, content: toWrite, session });
        dirty = view && view === v ? view.state.doc.toString() !== lastSaved : false;
      } catch (e) {
        dirty = true;
        toast.error(t('editor.saveFailed', { message: (e as Error).message }));
      }
    };
    if (now) await doSave();
    else void doSave();
  }

  function scheduleSave() {
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), settings.get<number>('editor.autosaveDelay'));
  }

  /** `[[Some Title]]` typed by hand → `[[cardId]]` when exactly one card matches. */
  async function resolveTitleLinks(v: View | null = view) {
    if (!v || readOnly) return;
    const doc = v.state.doc.toString();
    const found = [...doc.matchAll(/(!?)\[\[([^\[\]\n|#]+)((?:#[^\]\n|]*)?(?:\|[^\]\n]*)?)\]\]/g)].filter((m) => !/^c[a-z0-9]{6}$/.test(m[2].trim()));
    if (!found.length) return;
    const changes: { from: number; to: number; insert: string }[] = [];
    for (const m of found) {
      const title = m[2].trim();
      const hits = await rpc<SearchHit[]>('search.query', { q: `"${title.replace(/"/g, '')}" in:title`, opts: { limit: 5 } }).catch(() => []);
      const exact = hits.filter((h) => h.title.trim().toLowerCase() === title.toLowerCase());
      if (exact.length === 1) changes.push({ from: m.index!, to: m.index! + m[0].length, insert: `${m[1]}[[${exact[0].id}${m[3] ?? ''}]]` });
    }
    if (changes.length && v.state.doc.toString() === doc) v.dispatch({ changes, userEvent: 'input.resolveLinks' });
  }

  /** Save and close the edit session of the card `v` holds (defaults: current). */
  async function flushAndSeal(v: View | null = view, tg: Target | null = targetOf(v)) {
    if (!v || !tg) return;
    await resolveTitleLinks(v);
    await save(true, v, tg);
    await rpc('board.seal', { board: tg.boardId, card: tg.cardId }).catch(() => {});
    session = Math.random().toString(36).slice(2, 10);
  }

  function tagStyle(tag: string): string {
    const custom = settings.get<Record<string, string>>('tags.colors')?.[tag.toLowerCase()];
    const c = pastel(custom ? tagHue(custom) : tagHue(tag), theme.dark);
    return `--tag-bg:${c.bg};--tag-ink:${c.ink}`;
  }

  function attachment(ref: string): Attachment | undefined {
    const name = decodeURIComponent(ref.replace(/^\.\//, '')).split('/').pop();
    return model?.nodes.get(cardId)?.attachments.find((a) => a.file === name);
  }

  async function fileToBase64(f: File): Promise<string> {
    const buf = new Uint8Array(await f.arrayBuffer());
    let bin = '';
    for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
    return btoa(bin);
  }

  async function upload(f: File): Promise<string | null> {
    try {
      const name = f.name || `pasted.${(f.type.split('/')[1] || 'png').replace('jpeg', 'jpg')}`;
      const att = await rpc<Attachment>('attachment.add', { board: boardId, card: cardId, name, base64: await fileToBase64(f) });
      return att.file;
    } catch (e) {
      toast.error(t('editor.attachFailed', { message: (e as Error).message }));
      return null;
    }
  }

  function makeContext(): LuauEditorContext {
    return {
      boardId,
      cardId,
      fileUrl: (ref, w) => (model ? cardFileUrl(model, cardId, ref, w) : ref),
      attachment,
      titleOf: (id) => resolveTitle(id),
      isMissing: (id) => !!lookupCard(id)?.missing,
      openCard: (id) => void openCardById(id),
      openExternal: (url) => void openExternal(url),
      openFile: async (ref) => {
        if (/^https?:/i.test(ref)) return void openExternal(ref);
        if (!model || !isTauri) return;
        const dir = attachmentDirRel(model, cardId);
        const rel = (dir ? `${dir}/` : '') + decodeURIComponent(ref.replace(/^\.\//, ''));
        const abs = `${model.header.root}/${rel}`;
        const { openPath } = await import('@tauri-apps/plugin-opener');
        await openPath(abs).catch(() => reveal(abs));
      },
      renderEmbed: (id, _heading, el) => {
        let alive = true;
        void (async () => {
          let bid: string | undefined;
          for (const [b, m] of boards) if (m.nodes.has(id)) bid = b;
          if (!bid) bid = (await rpc<{ board: string } | null>('search.locate', { id }))?.board;
          if (!bid || !alive) {
            el.textContent = t('links.missing');
            return;
          }
          const content = await rpc<string>('card.read', { board: bid, id }).catch(() => '');
          const m = boards.get(bid);
          el.innerHTML = renderMarkdown(content, {
            titleOf: resolveTitle,
            fileUrl: (ref) => (m ? cardFileUrl(m, id, ref) : ref),
            tagStyle,
            formatDate: (d) => fmtDate(d),
            dropTitle: true,
          });
          void enhanceRendered(el);
        })();
        return () => (alive = false);
      },
      runCode: async (lang, code) => {
        try {
          // The backend checks the code is in the saved card: save first.
          await save(true);
          const tg = targetOf(view);
          const r = await runCodeCell(lang, code, tg?.boardId ?? boardId, tg?.cardId ?? cardId);
          outputs.set(`${lang}\u0000${code}`, r);
          return r;
        } catch (e) {
          return { ok: false, stdout: '', stderr: String((e as Error).message ?? e), images: [], ms: 0 };
        }
      },
      cachedOutput: (lang, code) => outputs.get(`${lang}\u0000${code}`) ?? null,
      linkPreview: (url) => {
        let p = previews.get(url);
        if (!p) {
          p = rpc<{ title: string; description: string; image: string | null; site: string } | null>('web.preview', { url }).catch(() => null);
          previews.set(url, p);
        }
        return p;
      },
      tagStyle,
      formatDate: (iso) => fmtDate(iso, iso.length > 10 ? { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' } : { month: 'short', day: 'numeric', year: iso.slice(0, 4) === String(new Date().getFullYear()) ? undefined : 'numeric' }),
      editFooter: () => {
        if (!view) return;
        const doc = view.state.doc.toString();
        const i = doc.lastIndexOf('\n---\n');
        view.dispatch({ selection: { anchor: i >= 0 ? i + 5 : doc.length } });
        view.focus();
      },
      t,
    };
  }

  async function mountEditor(content: string) {
    setup ??= await import('./cm/setup');
    const { EditorView } = await import('@codemirror/view');
    view?.destroy();
    lastSaved = content;
    dirty = false;
    const state = setup.createState({
      doc: content,
      ctx: makeContext(),
      placeholder: t('editor.placeholder'),
      livePreview: settings.get<boolean>('editor.livePreview'),
      vim: false,
      spellcheck: settings.get<boolean>('editor.spellcheck'),
      lineNumbers: settings.get<boolean>('editor.lineNumbers'),
      readOnly,
      autoPair: settings.get<boolean>('editor.autoPair'),
      tabSize: settings.get<number>('editor.tabSize'),
      completion: {
        tags: async () => (await rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => [])).map(([tag]) => tag),
        people: () => rpc<string[]>('search.people').catch(() => []),
        searchCards: async (q) => {
          const hits = await rpc<SearchHit[]>('search.query', { q: q.trim() ? `${q} in:title` : '', opts: { limit: 20 } }).catch(() => []);
          return hits.filter((h) => h.id !== cardId).map((h) => ({ id: h.id, title: h.title, board: h.boardName }));
        },
        createCard: async (title) => {
          const n = model?.nodes.get(cardId);
          if (!model || !n) return null;
          const { createCard } = await import('$lib/board/cardActions');
          const sibs = model.childrenOf(n.parent);
          return createCard(boardId, n.parent, sibs[sibs.indexOf(cardId) + 1] ?? null, `# ${title}\n`);
        },
        headings: async (id) => {
          for (const m of boards.values()) {
            const n = m.nodes.get(id);
            if (n) return n.headings.map((h) => h.text);
          }
          return [];
        },
        insertFile: async (v, from, to) => {
          v.dispatch({ changes: { from, to, insert: '' } });
          if (!isTauri) return;
          const paths = (await pickFile(t('slash.file'), undefined, true)) ?? [];
          const parts: string[] = [];
          for (const p of paths) {
            const name = p.split(/[\\/]/).pop() ?? 'file';
            const att = await rpc<Attachment>('attachment.add', { board: boardId, card: cardId, name, path: p }).catch(() => null);
            if (att) parts.push(fileMarkdown(name, att.file, att.kind === 'image' ? 'image/*' : ''));
          }
          if (parts.length) v.dispatch({ changes: { from, insert: parts.join(' ') }, selection: { anchor: from + parts.join(' ').length } });
        },
        t,
      },
      paste: { upload, richPaste: () => settings.get<boolean>('editor.pasteRich') },
      onChange: () => scheduleSave(),
      onFocus: (focused, v) => {
        if (focused) {
          const tg = targetOf(v);
          if (tg) setActiveEditor({ view: v, boardId: tg.boardId, cardId: tg.cardId });
          cmdCtx.editorFocus = true;
        } else {
          cmdCtx.editorFocus = false;
          void save(false, v);
        }
      },
      onSelection: (has) => (cmdCtx.editorHasSelection = has),
    });
    view = new EditorView({ state, parent: host! });
    if (target) targets.set(view, target);
    if (settings.get<boolean>('editor.vim')) await setup.applyVim(view, true);
    if (target) setActiveEditor({ view, boardId: target.boardId, cardId: target.cardId });
    if (autofocus) {
      requestAnimationFrame(() => {
        if (!view) return;
        view.focus();
        const first = view.state.doc.line(1);
        // New/empty title: caret at the end of the title line.
        const pos = /^#\s*$/.test(first.text) ? first.to : view.state.doc.length === 0 ? 0 : first.to;
        view.dispatch({ selection: { anchor: pos } });
      });
    }
  }

  async function load() {
    const key = `${boardId}/${cardId}`;
    if (key === loadedFor) return;
    if (view) await flushAndSeal(view);
    loadedFor = key;
    const next: Target = { boardId, cardId };
    const content = await rpc<string>('card.read', { board: next.boardId, id: next.cardId }).catch(() => '');
    if (loadedFor !== key) return;
    target = next;
    await mountEditor(content);
  }

  /** Replace the document with fresh disk content (keeps caret near). */
  function replaceDoc(content: string) {
    if (!view) return;
    const sel = view.state.selection.main;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: content },
      selection: { anchor: Math.min(sel.anchor, content.length), head: Math.min(sel.head, content.length) },
      annotations: [],
    });
    lastSaved = content;
    dirty = false;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
  }

  export function takeTheirs(content: string) {
    replaceDoc(content);
    onconflict?.(null);
  }

  export function keepMine() {
    onconflict?.(null);
    void save(true);
  }

  export async function flush() {
    await flushAndSeal();
  }

  onMount(() => {
    void load();
    const unload = () => void save(true);
    window.addEventListener('beforeunload', unload);
    return () => window.removeEventListener('beforeunload', unload);
  });

  // Card switched in place (sidebar following selection, editor history).
  $effect(() => {
    void boardId;
    void cardId;
    untrack(() => void load());
  });

  // Content changed outside this editor (external edit, face checkbox, commands).
  let lastMtime = 0;
  $effect(() => {
    const m = node?.mtime ?? 0;
    const ext = model?.externalTick ?? 0;
    void ext;
    untrack(() => {
      if (!view || !lastMtime) {
        lastMtime = m;
        return;
      }
      if (m === lastMtime) return;
      lastMtime = m;
      void rpc<string>('card.read', { board: boardId, id: cardId }).then((disk) => {
        if (!view) return;
        const current = view.state.doc.toString();
        if (disk === current || ensureTitle(current) === disk || disk === lastSaved || ensureTitle(lastSaved) === disk) return;
        if (!dirty) replaceDoc(disk);
        else onconflict?.({ mine: current, theirs: disk });
      });
    });
  });

  // Titles of linked cards changed → refresh chips.
  $effect(() => {
    let sig = 0;
    for (const m of boards.values()) sig += m.version;
    void sig;
    void i18n.locale;
    untrack(() => view?.dispatch({ effects: refreshPreview.of(null) }));
  });

  // Live settings.
  $effect(() => {
    const patch = {
      livePreview: settings.get<boolean>('editor.livePreview'),
      spellcheck: settings.get<boolean>('editor.spellcheck'),
      lineNumbers: settings.get<boolean>('editor.lineNumbers'),
      autoPair: settings.get<boolean>('editor.autoPair'),
      tabSize: settings.get<number>('editor.tabSize'),
      readOnly,
    };
    const vim = settings.get<boolean>('editor.vim');
    untrack(() => {
      if (view && setup) {
        setup.reconfigure(view, patch);
        void setup.applyVim(view, vim);
      }
    });
  });

  onDestroy(() => {
    if (view) {
      // Capture the view and its card: `view` is nulled right away so nothing
      // else touches it, but the final save still needs its document.
      const v = view;
      view = null;
      cmdCtx.editorFocus = false;
      void flushAndSeal(v).finally(() => {
        clearActiveEditor(v);
        v.destroy();
      });
    }
  });

</script>

<div class="luau-editor" class:full-width={fullWidth || settings.get<boolean>('editor.fullWidth')} bind:this={host}></div>

<style>
  .luau-editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    user-select: text;
  }
  .luau-editor :global(.cm-editor) {
    flex: 1;
    min-height: 0;
  }
</style>
