// Live-preview widgets. Each widget renders a calm, compact visual for a piece
// of Markdown syntax; clicking (or moving the cursor in) reveals the source.

import { WidgetType, EditorView } from '@codemirror/view';
import { luauContext, type CodeResult } from './context';

function ctxOf(view: EditorView) {
  return view.state.facet(luauContext);
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

const ICON = {
  file: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>',
  link: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>',
  play: '<svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor"><path d="M7 4v16l13-8z"/></svg>',
  copy: '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>',
  cal: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/></svg>',
  globe:
    '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/></svg>',
};

// --- tasks & lists ---------------------------------------------------------

export class CheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly from: number,
  ) {
    super();
  }
  eq(o: CheckboxWidget) {
    return o.checked === this.checked && o.from === this.from;
  }
  toDOM(view: EditorView) {
    const box = el('span', 'cm-task-box' + (this.checked ? ' on' : ''));
    box.setAttribute('role', 'checkbox');
    box.setAttribute('aria-checked', String(this.checked));
    box.onmousedown = (e) => {
      e.preventDefault();
      const text = view.state.sliceDoc(this.from, this.from + 3);
      if (!/^\[[ xX]\]$/.test(text)) return;
      view.dispatch({ changes: { from: this.from + 1, to: this.from + 2, insert: this.checked ? ' ' : 'x' }, userEvent: 'input.toggle' });
    };
    return box;
  }
  ignoreEvent() {
    return false;
  }
}

export class BulletWidget extends WidgetType {
  constructor(readonly depth: number) {
    super();
  }
  eq(o: BulletWidget) {
    return o.depth === this.depth;
  }
  toDOM() {
    return el('span', `cm-bullet d${this.depth % 3}`);
  }
}

export class HrWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    return el('span', 'cm-hr');
  }
}

// --- links, chips ------------------------------------------------------------

export class CardLinkWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly heading: string | null,
    readonly alias: string | null,
    readonly title: string,
    readonly missing: boolean,
  ) {
    super();
  }
  eq(o: CardLinkWidget) {
    return o.id === this.id && o.heading === this.heading && o.alias === this.alias && o.title === this.title && o.missing === this.missing;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const chip = el('span', 'cm-card-link' + (this.missing ? ' missing' : ''));
    chip.innerHTML = ICON.link;
    const label = this.alias || (this.heading ? `${this.title} › ${this.heading}` : this.title);
    chip.appendChild(el('span', '', label));
    chip.title = this.missing ? (c?.t('links.missing') ?? '') : label;
    chip.onmousedown = (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      if (e.altKey) return; // alt-click edits (cursor enters raw text)
      c?.openCard(this.id, this.heading);
    };
    return chip;
  }
  ignoreEvent(e: Event) {
    return e.type === 'mousedown' && !(e as MouseEvent).altKey;
  }
}

export class DateWidget extends WidgetType {
  constructor(readonly iso: string) {
    super();
  }
  eq(o: DateWidget) {
    return o.iso === this.iso;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const d = el('span', 'cm-date');
    const today = new Date().toISOString().slice(0, 10);
    if (this.iso.slice(0, 10) < today) d.classList.add('past');
    if (this.iso.slice(0, 10) === today) d.classList.add('today');
    d.innerHTML = ICON.cal;
    d.appendChild(el('span', '', c?.formatDate(this.iso) ?? this.iso));
    return d;
  }
}

export class FileChipWidget extends WidgetType {
  constructor(
    readonly ref: string,
    readonly label: string,
  ) {
    super();
  }
  eq(o: FileChipWidget) {
    return o.ref === this.ref && o.label === this.label;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const chip = el('span', 'cm-file-chip');
    chip.innerHTML = ICON.file;
    const att = c?.attachment(this.ref);
    chip.appendChild(el('span', 'name', this.label || att?.display || this.ref));
    if (att) chip.appendChild(el('span', 'size', fmtSize(att.size)));
    chip.onmousedown = (e) => {
      if (e.button !== 0 || e.altKey) return;
      e.preventDefault();
      c?.openFile(this.ref);
    };
    return chip;
  }
  ignoreEvent(e: Event) {
    return e.type === 'mousedown' && !(e as MouseEvent).altKey;
  }
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export class LinkPreviewWidget extends WidgetType {
  constructor(
    readonly url: string,
    readonly text: string,
  ) {
    super();
  }
  eq(o: LinkPreviewWidget) {
    return o.url === this.url && o.text === this.text;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const card = el('span', 'cm-link-preview');
    const body = el('span', 'body');
    const title = el('span', 'title', this.text || this.url);
    const desc = el('span', 'desc', '');
    const site = el('span', 'site');
    site.innerHTML = ICON.globe;
    try {
      site.appendChild(el('span', '', new URL(this.url).hostname));
    } catch {
      /* ignore */
    }
    body.append(title, desc, site);
    card.appendChild(body);
    card.onmousedown = (e) => {
      if (e.button !== 0 || e.altKey) return;
      e.preventDefault();
      c?.openExternal(this.url);
    };
    void c?.linkPreview(this.url).then((p) => {
      if (!p) return;
      if (p.title) title.textContent = p.title;
      desc.textContent = p.description;
      if (p.image) {
        const img = el('img', 'thumb');
        img.referrerPolicy = 'no-referrer';
        img.src = p.image;
        img.alt = '';
        img.loading = 'lazy';
        card.insertBefore(img, body);
      }
    });
    return card;
  }
  ignoreEvent(e: Event) {
    return e.type === 'mousedown' && !(e as MouseEvent).altKey;
  }
}

// --- media -----------------------------------------------------------------

/** Inline image with a resize handle; width is stored as `![alt|W](src)`. */
export class ImageWidget extends WidgetType {
  constructor(
    readonly ref: string,
    readonly alt: string,
    readonly width: number | null,
    readonly from: number,
    readonly to: number,
  ) {
    super();
  }
  eq(o: ImageWidget) {
    return o.ref === this.ref && o.alt === this.alt && o.width === this.width && o.from === this.from && o.to === this.to;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const wrap = el('span', 'cm-image');
    const img = el('img');
    img.referrerPolicy = 'no-referrer';
    const w = this.width ?? 420;
    img.src = c?.fileUrl(this.ref, Math.max(w, 160)) ?? this.ref;
    img.alt = this.alt;
    img.draggable = false;
    img.loading = 'lazy';
    if (this.width) wrap.style.width = `${this.width}px`;
    img.onerror = () => wrap.classList.add('broken');
    wrap.appendChild(img);
    const handle = el('span', 'handle');
    wrap.appendChild(handle);
    img.onmousedown = (e) => {
      if (e.button !== 0 || e.altKey) return;
      e.preventDefault();
      if (e.detail >= 2) c?.openFile(this.ref);
    };
    handle.onpointerdown = (e) => {
      e.preventDefault();
      e.stopPropagation();
      const startX = e.clientX;
      const startW = wrap.getBoundingClientRect().width;
      handle.setPointerCapture(e.pointerId);
      const move = (ev: PointerEvent) => {
        wrap.style.width = `${Math.max(48, Math.round(startW + ev.clientX - startX))}px`;
      };
      const up = () => {
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', up);
        const nw = Math.round(wrap.getBoundingClientRect().width);
        const src = view.state.sliceDoc(this.from, this.to);
        const m = /^!\[([^\]|]*)(?:\|\d+)?\]\((.*)\)$/s.exec(src);
        if (m) view.dispatch({ changes: { from: this.from, to: this.to, insert: `![${m[1]}|${nw}](${m[2]})` }, userEvent: 'input.resize' });
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', up);
    };
    return wrap;
  }
  ignoreEvent(e: Event) {
    return e.type !== 'mousedown' || !(e as MouseEvent).altKey;
  }
}

export class PdfWidget extends WidgetType {
  constructor(
    readonly ref: string,
    readonly width: number | null,
  ) {
    super();
  }
  eq(o: PdfWidget) {
    return o.ref === this.ref && o.width === this.width;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const wrap = el('span', 'cm-image cm-pdf');
    if (this.width) wrap.style.width = `${this.width}px`;
    const canvas = el('canvas');
    wrap.appendChild(canvas);
    const cap = el('span', 'caption');
    cap.innerHTML = ICON.file;
    cap.appendChild(el('span', '', c?.attachment(this.ref)?.display ?? this.ref));
    wrap.appendChild(cap);
    wrap.onmousedown = (e) => {
      if (e.button !== 0 || e.altKey) return;
      e.preventDefault();
      c?.openFile(this.ref);
    };
    const url = c?.fileUrl(this.ref) ?? this.ref;
    void renderPdfThumb(url, canvas, this.width ?? 260).catch(() => wrap.classList.add('broken'));
    return wrap;
  }
  ignoreEvent() {
    return true;
  }
}

async function renderPdfThumb(url: string, canvas: HTMLCanvasElement, width: number) {
  const pdfjs = await import('pdfjs-dist');
  const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
  pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
  const task = pdfjs.getDocument({ url });
  const doc = await task.promise;
  const page = await doc.getPage(1);
  const vp0 = page.getViewport({ scale: 1 });
  const scale = (width * (window.devicePixelRatio || 1)) / vp0.width;
  const vp = page.getViewport({ scale });
  canvas.width = vp.width;
  canvas.height = vp.height;
  canvas.style.width = '100%';
  await page.render({ canvas, canvasContext: canvas.getContext('2d')!, viewport: vp }).promise;
  void task.destroy();
}

export class MediaWidget extends WidgetType {
  constructor(
    readonly ref: string,
    readonly kind: 'video' | 'audio',
  ) {
    super();
  }
  eq(o: MediaWidget) {
    return o.ref === this.ref && o.kind === this.kind;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const m = document.createElement(this.kind);
    m.className = `cm-media ${this.kind}`;
    m.controls = true;
    m.preload = 'metadata';
    m.src = c?.fileUrl(this.ref) ?? this.ref;
    return m;
  }
  ignoreEvent() {
    return true;
  }
}

// --- blocks ------------------------------------------------------------------

export class CalloutLabelWidget extends WidgetType {
  constructor(
    readonly kind: string,
    readonly label: string,
  ) {
    super();
  }
  eq(o: CalloutLabelWidget) {
    return o.kind === this.kind && o.label === this.label;
  }
  toDOM() {
    const s = el('span', `cm-callout-label k-${this.kind}`);
    s.appendChild(el('span', 'dot'));
    s.appendChild(el('span', '', this.label));
    return s;
  }
}

export class CodeHeaderWidget extends WidgetType {
  constructor(
    readonly lang: string,
    readonly code: string,
    readonly runnable: boolean,
  ) {
    super();
  }
  eq(o: CodeHeaderWidget) {
    return o.lang === this.lang && o.code === this.code && o.runnable === this.runnable;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const head = el('span', 'cm-code-head');
    head.appendChild(el('span', 'lang', this.lang || 'text'));
    const spacer = el('span', 'grow');
    head.appendChild(spacer);
    if (this.runnable) {
      const run = el('button', 'run');
      run.innerHTML = `${ICON.play}<span>${c?.t('editor.run') ?? 'Run'}</span>`;
      run.onmousedown = (e) => {
        e.preventDefault();
        e.stopPropagation();
        view.dispatch({ effects: runRequest.of({ lang: this.lang, code: this.code }) });
      };
      head.appendChild(run);
    }
    const copy = el('button', 'copy');
    copy.innerHTML = ICON.copy;
    copy.title = c?.t('common.copy') ?? 'Copy';
    copy.onmousedown = (e) => {
      e.preventDefault();
      e.stopPropagation();
      void navigator.clipboard.writeText(this.code);
      copy.classList.add('done');
      setTimeout(() => copy.classList.remove('done'), 900);
    };
    head.appendChild(copy);
    return head;
  }
  ignoreEvent() {
    return true;
  }
}

import { StateEffect } from '@codemirror/state';
export const runRequest = StateEffect.define<{ lang: string; code: string }>();
export const outputsChanged = StateEffect.define<null>();

export class CodeOutputWidget extends WidgetType {
  constructor(
    readonly lang: string,
    readonly code: string,
    readonly result: CodeResult | 'running',
  ) {
    super();
  }
  eq(o: CodeOutputWidget) {
    return o.lang === this.lang && o.code === this.code && o.result === this.result;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const box = el('div', 'cm-code-output');
    if (this.result === 'running') {
      box.classList.add('running');
      box.appendChild(el('span', 'muted', c?.t('editor.running') ?? 'Running…'));
      return box;
    }
    const r = this.result;
    const meta = el('div', 'meta', `${r.ok ? '✓' : '✕'} ${r.ms} ms`);
    if (!r.ok) meta.classList.add('err');
    box.appendChild(meta);
    if (r.stdout) box.appendChild(el('pre', 'out', r.stdout));
    if (r.stderr) box.appendChild(el('pre', 'err', r.stderr));
    for (const img of r.images) {
      const i = el('img');
      i.referrerPolicy = 'no-referrer';
      i.src = `data:image/png;base64,${img}`;
      i.alt = '';
      box.appendChild(i);
    }
    return box;
  }
  get estimatedHeight() {
    return 60;
  }
  ignoreEvent() {
    return true;
  }
}

let katexReady: Promise<typeof import('katex')> | null = null;
function loadKatex() {
  if (!katexReady) {
    katexReady = Promise.all([import('katex'), import('katex/dist/katex.min.css')]).then(([k]) => k);
  }
  return katexReady;
}

export class MathWidget extends WidgetType {
  constructor(
    readonly tex: string,
    readonly display: boolean,
  ) {
    super();
  }
  eq(o: MathWidget) {
    return o.tex === this.tex && o.display === this.display;
  }
  toDOM() {
    const span = el(this.display ? 'div' : 'span', this.display ? 'cm-math-block' : 'cm-math');
    span.textContent = this.tex;
    void loadKatex().then((k) => {
      try {
        (k.default ?? k).render(this.tex, span, { displayMode: this.display, throwOnError: false, trust: false, strict: 'ignore' });
      } catch {
        span.classList.add('broken');
      }
    });
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

let mermaidReady: Promise<(typeof import('mermaid'))['default']> | null = null;
let mermaidSeq = 0;
let mermaidDark: boolean | null = null;
function loadMermaid(dark: boolean) {
  mermaidReady ??= import('mermaid').then((m) => m.default);
  return mermaidReady.then((m) => {
    // Re-initialize when the theme changed since the last diagram.
    if (mermaidDark !== dark) {
      m.initialize({ startOnLoad: false, securityLevel: 'strict', theme: dark ? 'dark' : 'neutral', fontFamily: 'Inter Variable, system-ui' });
      mermaidDark = dark;
    }
    return m;
  });
}

export class MermaidWidget extends WidgetType {
  readonly dark = document.documentElement.dataset.theme === 'dark';
  constructor(readonly code: string) {
    super();
  }
  eq(o: MermaidWidget) {
    return o.code === this.code && o.dark === this.dark;
  }
  toDOM() {
    const box = el('div', 'cm-mermaid');
    box.textContent = '…';
    const dark = this.dark;
    void loadMermaid(dark).then(async (m) => {
      try {
        const { svg } = await m.render(`luau-mermaid-${++mermaidSeq}`, this.code);
        box.innerHTML = svg;
      } catch (e) {
        box.classList.add('broken');
        box.textContent = String((e as Error).message ?? e);
      }
    });
    return box;
  }
  get estimatedHeight() {
    return 200;
  }
  ignoreEvent() {
    return false;
  }
}

export class TableWidget extends WidgetType {
  constructor(readonly source: string) {
    super();
  }
  eq(o: TableWidget) {
    return o.source === this.source;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const wrap = el('div', 'cm-table-wrap');
    const table = el('table', 'cm-table');
    const rows = this.source
      .trim()
      .split('\n')
      .map((l) =>
        l
          .trim()
          .replace(/^\||\|$/g, '')
          .split(/(?<!\\)\|/)
          .map((x) => x.trim()),
      );
    const align = (rows[1] ?? []).map((s) => (s.startsWith(':') && s.endsWith(':') ? 'center' : s.endsWith(':') ? 'right' : 'left'));
    const head = el('thead');
    const htr = el('tr');
    (rows[0] ?? []).forEach((h, i) => {
      const th = el('th');
      th.style.textAlign = align[i] ?? 'left';
      renderInline(th, h, c);
      htr.appendChild(th);
    });
    head.appendChild(htr);
    table.appendChild(head);
    const body = el('tbody');
    for (const r of rows.slice(2)) {
      const tr = el('tr');
      r.forEach((cell, i) => {
        const td = el('td');
        td.style.textAlign = align[i] ?? 'left';
        renderInline(td, cell, c);
        tr.appendChild(td);
      });
      body.appendChild(tr);
    }
    table.appendChild(body);
    wrap.appendChild(table);
    return wrap;
  }
  ignoreEvent() {
    return false;
  }
}

/** Minimal safe inline renderer for widget contents (no HTML injection). */
export function renderInline(target: HTMLElement, text: string, c: ReturnType<typeof ctxOf>) {
  const re = /(\*\*[^*]+\*\*|\*[^*\s][^*]*\*|`[^`]+`|~~[^~]+~~|==[^=]+==|\[\[[^\]]+\]\]|\[[^\]]*\]\([^)]*\)|#[\p{L}\p{N}_/-]+)/gu;
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index! > last) target.appendChild(document.createTextNode(text.slice(last, m.index)));
    const s = m[0];
    if (s.startsWith('**')) target.appendChild(el('strong', '', s.slice(2, -2)));
    else if (s.startsWith('`')) target.appendChild(el('code', '', s.slice(1, -1)));
    else if (s.startsWith('~~')) target.appendChild(el('s', '', s.slice(2, -2)));
    else if (s.startsWith('==')) target.appendChild(el('mark', '', s.slice(2, -2)));
    else if (s.startsWith('[[')) {
      const id = s.slice(2, -2).split('|')[0].split('#')[0];
      target.appendChild(el('span', 'cm-card-link inline', c?.titleOf(id) ?? id));
    } else if (s.startsWith('[')) target.appendChild(el('span', 'cm-link-text', s.slice(1, s.indexOf(']('))));
    else if (s.startsWith('#')) {
      const t = el('span', 'cm-tag', s);
      if (c) t.setAttribute('style', c.tagStyle(s.slice(1)));
      target.appendChild(t);
    } else target.appendChild(el('em', '', s.slice(1, -1)));
    last = m.index! + s.length;
  }
  if (last < text.length) target.appendChild(document.createTextNode(text.slice(last)));
}

export class EmbedWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly heading: string | null,
  ) {
    super();
  }
  eq(o: EmbedWidget) {
    return o.id === this.id && o.heading === this.heading;
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const box = el('div', 'cm-embed');
    const head = el('div', 'cm-embed-head');
    head.innerHTML = ICON.link;
    head.appendChild(el('span', '', c?.titleOf(this.id) ?? this.id));
    head.onmousedown = (e) => {
      e.preventDefault();
      c?.openCard(this.id, this.heading);
    };
    const body = el('div', 'cm-embed-body luau-md');
    box.append(head, body);
    const cleanup = c?.renderEmbed(this.id, this.heading, body);
    (box as unknown as { _cleanup?: () => void })._cleanup = cleanup;
    return box;
  }
  destroy(dom: HTMLElement) {
    (dom as unknown as { _cleanup?: () => void })._cleanup?.();
  }
  get estimatedHeight() {
    return 120;
  }
  ignoreEvent() {
    return true;
  }
}

export class PropertiesWidget extends WidgetType {
  constructor(readonly fields: [string, string][]) {
    super();
  }
  eq(o: PropertiesWidget) {
    return JSON.stringify(o.fields) === JSON.stringify(this.fields);
  }
  toDOM(view: EditorView) {
    const c = ctxOf(view);
    const box = el('div', 'cm-properties');
    for (const [k, v] of this.fields) {
      const row = el('span', `prop k-${k}`);
      row.appendChild(el('span', 'key', c?.t(`properties.${k}`) === `properties.${k}` ? k : (c?.t(`properties.${k}`) ?? k)));
      const val = el('span', 'val');
      if (k === 'due' || k === 'start') val.textContent = c?.formatDate(v) ?? v;
      else if (k === 'priority') {
        const p = v.trim().toLowerCase();
        val.classList.add(`p-${p}`);
        const label = c?.t(`priority.${p}`);
        val.textContent = label && label !== `priority.${p}` ? label : v;
      } else if (k === 'labels' || k === 'assignees') {
        for (const item of v
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean)) {
          const chip = el('span', k === 'assignees' ? 'cm-mention' : 'cm-tag', item);
          if (k === 'labels' && c) chip.setAttribute('style', c.tagStyle(item.replace(/^#/, '')));
          val.appendChild(chip);
        }
      } else val.textContent = v;
      row.appendChild(val);
      box.appendChild(row);
    }
    box.onmousedown = (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      c?.editFooter();
    };
    return box;
  }
  ignoreEvent() {
    return true;
  }
}
