// Built-in guide boards: one kanban, one files board. Each card is a Markdown
// file in ./cards that shows one feature. Card text may use `{{card:key}}`
// (another guide card, turned into its id by the core) and `{{asset:name}}`
// (a sample file attached to that card). Loaded lazily: the content is only
// needed when someone creates a guide board.

import type { TemplateAsset, TemplateSpec } from '../io';

const cards = import.meta.glob('./cards/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
const binary = import.meta.glob('./*.{png,pdf}', { query: '?inline', import: 'default', eager: true }) as Record<string, string>;
const svgs = import.meta.glob('./*.svg', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

export const GUIDE_TEMPLATES = ['guideKanban', 'guideNotes'] as const;
export type GuideTemplate = (typeof GUIDE_TEMPLATES)[number];

/** Card files (without `.md`) by key; the key is what `{{card:key}}` uses. */
const FILES: Record<string, string> = {
  welcomeKanban: 'welcome-kanban',
  welcomeNotes: 'welcome-notes',
  boardBasics: 'board-basics',
  filesBasics: 'files-basics',
  tryDrag: 'try-drag',
  tryTarget: 'try-target',
  text: 'text',
  lists: 'lists',
  tables: 'tables',
  links: 'links',
  embedMe: 'embed-me',
  code: 'code',
  math: 'math',
  diagrams: 'diagrams',
  callouts: 'callouts',
  images: 'images',
  tags: 'tags',
  properties: 'properties',
  search: 'search',
  shortcuts: 'shortcuts',
  archiveMe: 'archive-me',
};

/** Sample files attached to cards (by card key). */
const ASSETS: Record<string, string[]> = {
  images: ['sunset.svg', 'luau.png', 'quick-reference.pdf'],
  links: ['quick-reference.pdf'],
};

const TOPICS = ['text', 'lists', 'tables', 'links', 'embedMe', 'code', 'math', 'diagrams', 'callouts', 'images', 'tags', 'properties', 'search', 'shortcuts'];

/** Lanes of the kanban guide: [lane name, card keys]. */
const KANBAN_LANES: [string, string[]][] = [
  ['👋 Start here', ['welcomeKanban', 'boardBasics', 'tryDrag', 'tryTarget']],
  ['✍️ Writing', ['text', 'lists', 'tables', 'links', 'embedMe']],
  ['🧩 Rich blocks', ['code', 'math', 'diagrams', 'callouts', 'images']],
  ['🏷 Organizing', ['tags', 'properties', 'search', 'shortcuts']],
  ['✅ Done', ['archiveMe']],
];

const NOTES_ORDER = ['welcomeNotes', 'filesBasics', ...TOPICS];

function card(key: string): string {
  const text = cards[`./cards/${FILES[key]}.md`];
  if (text === undefined) throw new Error(`missing guide card ${key}`);
  return text;
}

function base64Of(name: string): string {
  const svg = svgs[`./${name}`];
  if (svg !== undefined) return btoa(String.fromCharCode(...new TextEncoder().encode(svg)));
  const url = binary[`./${name}`];
  if (!url) throw new Error(`missing guide file ${name}`);
  return url.slice(url.indexOf(',') + 1); // data:<mime>;base64,<data>
}

/** `{{card:key}}` → `{{card:N}}` (N = position in the template). */
function numbered(text: string, order: string[]): string {
  return text.replace(/\{\{card:([A-Za-z]+)\}\}/g, (m, key: string) => {
    const i = order.indexOf(key);
    return i < 0 ? m : `{{card:${i}}}`;
  });
}

function assetsFor(order: string[]): TemplateAsset[] {
  return order.flatMap((key, i) => (ASSETS[key] ?? []).map((name) => ({ card: i, name, data: base64Of(name) })));
}

export function guideSpec(id: GuideTemplate): TemplateSpec {
  if (id === 'guideKanban') {
    const order = KANBAN_LANES.flatMap(([, keys]) => keys);
    return {
      kind: 'kanban',
      lanes: KANBAN_LANES.map(([name, keys]) => ({ name, cards: keys.map((k) => numbered(card(k), order)) })),
      notes: [],
      assets: assetsFor(order),
    };
  }
  // Documents are listed by title, so they are numbered to read in order.
  const notes = NOTES_ORDER.map((k, i) => numbered(card(k), NOTES_ORDER).replace(/^# /, `# ${i + 1} · `));
  return { kind: 'files', lanes: [], notes, assets: assetsFor(NOTES_ORDER) };
}
