// Command palette & quick open (single widget, prefix-routed like VS Code):
//   (none) quick open cards/docs/boards · '>' commands · '#' tags · '@' lanes/headings · '?' help

import { FileText, LayoutGrid, Hash, Columns3, Heading, Terminal, CircleHelp, Diamond, Clock } from '@lucide/svelte';
import { quickPick, type QuickItem } from './qi.svelte';
import { allCommands, commandTitle, commandCategory, isEnabled, recentCommands, runCommand } from '$lib/commands/registry.svelte';
import { primaryKey } from '$lib/keybindings/resolver.svelte';
import { fuzzy } from '$lib/util/fuzzy';
import { rpc } from '$lib/backend/rpc';
import type { SearchHit } from '$lib/backend/types';
import { registry } from '$lib/state/registry.svelte';
import { uiGet } from '$lib/state/persist.svelte';
import { boards } from '$lib/state/boards.svelte';
import { openBoardTab } from '$lib/state/workspace.svelte';
import { activeBoard } from '$lib/app/helpers';
import { openCard } from '$lib/app/open';
import { ui } from '$lib/state/ui.svelte';
import { t } from '$lib/i18n/index.svelte';

type Action = () => void | Promise<void>;

function rank(items: QuickItem<Action>[], q: string): QuickItem<Action>[] {
  if (!q) return items;
  const scored: { it: QuickItem<Action>; s: number }[] = [];
  for (const it of items) {
    const m = fuzzy(q, it.label);
    const d = !m && it.description ? fuzzy(q, it.description) : null;
    if (m || d) scored.push({ it: { ...it, highlights: m?.positions ?? [] }, s: m ? m.score : d!.score - 60 });
  }
  return scored.sort((a, b) => b.s - a.s).map((x) => x.it);
}

function commandItems(q: string): QuickItem<Action>[] {
  const recent = recentCommands();
  const list = allCommands()
    .filter((c) => !c.hidden && isEnabled(c))
    .map((c) => {
      const cat = commandCategory(c);
      return {
        label: cat ? `${cat}: ${commandTitle(c)}` : commandTitle(c),
        keybinding: primaryKey(c.id),
        value: () => void runCommand(c.id),
        icon: c.icon,
        _id: c.id,
      } as QuickItem<Action> & { _id: string };
    });
  if (!q) {
    const rec = recent.map((id) => list.find((x) => (x as { _id: string })._id === id)).filter(Boolean) as QuickItem<Action>[];
    const rest = list.filter((x) => !recent.includes((x as { _id: string })._id)).sort((a, b) => a.label.localeCompare(b.label));
    return [
      ...(rec.length
        ? [
            { kind: 'separator', label: t('palette.recent') } as QuickItem<Action>,
            ...rec,
            { kind: 'separator', label: t('palette.allCommands') } as QuickItem<Action>,
          ]
        : []),
      ...rest,
    ];
  }
  return rank(list, q);
}

async function tagItems(q: string): Promise<QuickItem<Action>[]> {
  const tags = await rpc<[string, number][]>('search.tags', { boards: [] });
  return rank(
    tags.map(([tag, n]) => ({
      label: `#${tag}`,
      description: t('palette.cardsCount', { count: n }),
      icon: Hash,
      value: () => {
        ui.left.visible = true;
        ui.left.section = 'search';
        ui.searchQuery = `tag:${tag}`;
        ui.searchFocus++;
      },
    })),
    q,
  );
}

function outlineItems(q: string): QuickItem<Action>[] {
  const b = activeBoard();
  if (!b) return [];
  if (ui.editor.open || b.kind === 'files') {
    const id = ui.editor.open ? ui.editor.cardId : null;
    const n = id ? b.node(id) : null;
    if (n) {
      return rank(
        n.headings.map((h) => ({
          label: `${'  '.repeat(h.level - 1)}${h.text}`,
          icon: Heading,
          value: () => void import('$lib/editor/active').then((m) => m.scrollEditorToLine(h.line)),
        })),
        q,
      );
    }
  }
  return rank(
    b.lanes.map((l) => ({
      label: l.name,
      description: t('palette.cardsCount', { count: l.order.length }),
      icon: Columns3,
      value: () => {
        document.querySelector(`[data-lane="${l.id}"]`)?.scrollIntoView({ behavior: 'smooth', inline: 'center', block: 'nearest' });
      },
    })),
    q,
  );
}

async function openItems(q: string): Promise<QuickItem<Action>[]> {
  const boardItems: QuickItem<Action>[] = registry.data.boards
    .filter((b) => !b.hidden)
    .map((b) => ({
      label: b.name,
      description: b.mirror ? t('palette.mirror') : b.kind === 'files' ? t('palette.filesBoard') : t('palette.board'),
      icon: b.mirror ? Diamond : b.kind === 'files' ? FileText : LayoutGrid,
      value: () => void openBoardTab(b.id),
    }));
  if (!q) {
    const recent = uiGet<{ boardId: string; cardId: string }[]>('recentCards', []).slice(0, 8);
    const rec: QuickItem<Action>[] = recent.flatMap((r) => {
      const n = boards.get(r.boardId)?.node(r.cardId);
      return n
        ? [
            {
              label: n.title || t('common.untitled'),
              description: boards.get(r.boardId)?.header.name,
              icon: Clock,
              value: () => void openCard(r.boardId, r.cardId),
            },
          ]
        : [];
    });
    return [
      ...(rec.length ? [{ kind: 'separator', label: t('palette.recent') } as QuickItem<Action>, ...rec] : []),
      { kind: 'separator', label: t('palette.boards') } as QuickItem<Action>,
      ...boardItems,
    ];
  }
  const hits = await rpc<SearchHit[]>('search.query', { q: `${q} in:title`, opts: { limit: 40 } }).catch(() => [] as SearchHit[]);
  const cardItems: QuickItem<Action>[] = hits.map((h) => ({
    label: h.title || t('common.untitled'),
    description: [h.remoteKey, h.boardName, h.laneName].filter(Boolean).join(' · '),
    icon: h.kind === 'doc' ? FileText : LayoutGrid,
    value: () => void openCard(h.board, h.id),
  }));
  return [...rank(boardItems, q).slice(0, 5), ...rank(cardItems, q)];
}

function helpItems(): QuickItem<Action>[] {
  const mk = (prefix: string, key: string): QuickItem<Action> => ({
    label: `${prefix || '…'}  ${t(key)}`,
    icon: CircleHelp,
    value: () => openPalette(prefix),
  });
  return [mk('', 'palette.help.open'), mk('>', 'palette.help.commands'), mk('#', 'palette.help.tags'), mk('@', 'palette.help.outline')];
}

export async function openPalette(initial = '') {
  const res = await quickPick<Action>([], {
    value: initial,
    // A mode prefix ('>', '#', '@') must survive the first keystroke: caret after it, nothing selected.
    selectAll: false,
    placeholder: t('palette.placeholder'),
    selfFiltered: true,
    onValue: async (text) => {
      if (text.startsWith('>')) return commandItems(text.slice(1).trim());
      if (text.startsWith('#')) return tagItems(text.slice(1).trim());
      if (text.startsWith('@')) return outlineItems(text.slice(1).trim());
      if (text.startsWith('?')) return helpItems();
      return openItems(text.trim());
    },
  });
  if (typeof res === 'function') await (res as Action)();
}

export { Terminal as PaletteIcon };
