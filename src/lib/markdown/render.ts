// Markdown → HTML (preview mode, embeds, HTML/PDF export). Raw HTML is
// disabled so imported/remote content can never inject markup or scripts.

import MarkdownItFactory, { type MarkdownIt, type StateInline, type StateBlock } from 'markdown-it';
import footnote from 'markdown-it-footnote';
import { parseFooter } from './meta';

export interface RenderOptions {
  titleOf: (id: string) => string;
  fileUrl: (ref: string) => string;
  tagStyle?: (tag: string) => string;
  formatDate?: (iso: string) => string;
  /** Skip the first `# Title` line (it's shown separately). */
  dropTitle?: boolean;
  /** Render property footer as a table. */
  footerTable?: boolean;
}

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

function inlineRule(name: string, re: RegExp, prev: (s: string, pos: number) => boolean) {
  return (state: StateInline, silent: boolean) => {
    const src = state.src.slice(state.pos);
    const m = re.exec(src);
    if (!m || m.index !== 0 || !prev(state.src, state.pos)) return false;
    if (!silent) {
      const tok = state.push(name, '', 0);
      tok.content = m[0];
      tok.meta = m as unknown as Record<string, unknown>;
    }
    state.pos += m[0].length;
    return true;
  };
}

const wordBoundary = (s: string, pos: number) => pos === 0 || /[\s(\[,;]/.test(s[pos - 1]);

export function createRenderer(o: RenderOptions): MarkdownIt {
  const md = new MarkdownItFactory({ html: false, linkify: true, typographer: false, breaks: false });
  md.use(footnote);

  md.inline.ruler.before('link', 'card_link', inlineRule('card_link', /^!?\[\[([^\]\n|#]+)(?:#([^\]\n|]+))?(?:\|([^\]\n]+))?\]\]/, () => true));
  md.inline.ruler.before('link', 'date_chip', inlineRule('date_chip', /^\[(\d{4}-\d{2}-\d{2})(?:[ T]\d{1,2}:\d{2})?\](?![([:\]])/, (s, p) => s[p - 1] !== '['));
  md.inline.ruler.push('tag', inlineRule('tag', /^#([\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/u, wordBoundary));
  md.inline.ruler.push('mention', inlineRule('mention', /^@([\p{L}\p{N}][\p{L}\p{N}_.-]*)/u, wordBoundary));
  md.inline.ruler.before('emphasis', 'highlight', inlineRule('highlight', /^==([^=\n]+)==/, () => true));
  md.inline.ruler.before('escape', 'math_inline', inlineRule('math_inline', /^\$([^$\s](?:[^$\n]*[^$\s])?)\$(?!\d)/, (s, p) => s[p - 1] !== '\\'));

  md.block.ruler.before('fence', 'math_block', (state: StateBlock, start: number, end: number, silent: boolean) => {
    const pos = state.bMarks[start] + state.tShift[start];
    if (state.src.slice(pos, pos + 2) !== '$$') return false;
    let line = start;
    const first = state.src.slice(pos + 2, state.eMarks[start]);
    let content = '';
    if (first.trim().endsWith('$$') && first.trim().length > 2) content = first.trim().slice(0, -2);
    else {
      content = first;
      while (++line < end) {
        const l = state.src.slice(state.bMarks[line] + state.tShift[line], state.eMarks[line]);
        if (l.trim() === '$$') break;
        content += '\n' + l;
      }
    }
    if (silent) return true;
    const tok = state.push('math_block', 'div', 0);
    tok.content = content.trim();
    state.line = line + 1;
    return true;
  });

  md.renderer.rules.card_link = (tokens, i) => {
    const m = tokens[i].meta as unknown as RegExpExecArray;
    const id = m[1].trim();
    const label = m[3]?.trim() || (m[2] ? `${o.titleOf(id)} › ${m[2]}` : o.titleOf(id));
    return `<span class="card-link" data-card="${esc(id)}">${esc(label)}</span>`;
  };
  md.renderer.rules.date_chip = (tokens, i) => {
    const iso = (tokens[i].meta as unknown as RegExpExecArray)[1];
    return `<span class="date-chip">${esc(o.formatDate?.(iso) ?? iso)}</span>`;
  };
  md.renderer.rules.tag = (tokens, i) => {
    const tag = (tokens[i].meta as unknown as RegExpExecArray)[1];
    return `<span class="tag" style="${esc(o.tagStyle?.(tag) ?? '')}">#${esc(tag)}</span>`;
  };
  md.renderer.rules.mention = (tokens, i) => `<span class="mention">@${esc((tokens[i].meta as unknown as RegExpExecArray)[1])}</span>`;
  md.renderer.rules.highlight = (tokens, i) => `<mark>${esc((tokens[i].meta as unknown as RegExpExecArray)[1])}</mark>`;
  md.renderer.rules.math_inline = (tokens, i) => `<span class="math" data-tex="${esc((tokens[i].meta as unknown as RegExpExecArray)[1])}">${esc((tokens[i].meta as unknown as RegExpExecArray)[1])}</span>`;
  md.renderer.rules.math_block = (tokens, i) => `<div class="math-block" data-tex="${esc(tokens[i].content)}">${esc(tokens[i].content)}</div>`;

  const fence = md.renderer.rules.fence!;
  md.renderer.rules.fence = (tokens, i, opts, env, self) => {
    const info = tokens[i].info.trim().toLowerCase();
    if (info === 'mermaid') return `<div class="mermaid-src" data-code="${esc(tokens[i].content)}"></div>`;
    return fence(tokens, i, opts, env, self);
  };

  // Task list items.
  md.core.ruler.after('inline', 'tasks', (state) => {
    const toks = state.tokens;
    for (let i = 2; i < toks.length; i++) {
      const tk = toks[i];
      if (tk.type !== 'inline' || toks[i - 1].type !== 'paragraph_open' || toks[i - 2].type !== 'list_item_open') continue;
      const m = /^\[([ xX])\]\s/.exec(tk.content);
      if (!m) continue;
      toks[i - 2].attrJoin('class', 'task-list-item');
      const first = tk.children?.[0];
      if (first && first.type === 'text') first.content = first.content.replace(/^\[[ xX]\]\s/, '');
      const box = new state.Token('html_inline', '', 0);
      box.content = `<input type="checkbox" disabled${m[1] !== ' ' ? ' checked' : ''}> `;
      tk.children?.unshift(box);
    }
  });

  // Callouts: > [!note] Title
  md.core.ruler.after('block', 'callouts', (state) => {
    const toks = state.tokens;
    for (let i = 0; i < toks.length; i++) {
      if (toks[i].type !== 'blockquote_open') continue;
      const inline = toks[i + 2];
      if (inline?.type !== 'inline') continue;
      const m = /^\[!(\w+)\][+-]?\s*(.*)/.exec(inline.content);
      if (!m) continue;
      toks[i].attrJoin('class', `callout k-${m[1].toLowerCase()}`);
      inline.content = inline.content.replace(/^\[!\w+\][+-]?\s*/, m[2] ? '**' : '') + (m[2] ? '**' : '');
    }
  });

  // Images and links: resolve local files.
  const image = md.renderer.rules.image!;
  md.renderer.rules.image = (tokens, i, opts, env, self) => {
    const tk = tokens[i];
    const src = String(tk.attrGet('src') ?? '');
    const alt = tk.content;
    const [a, w] = alt.split('|');
    if (!/^[a-z]+:/i.test(src)) tk.attrSet('src', o.fileUrl(decodeURIComponent(src)));
    if (w && /^\d+$/.test(w)) tk.attrSet('width', w);
    tk.content = a;
    if (tk.children?.[0]) tk.children[0].content = a;
    return image(tokens, i, opts, env, self);
  };
  const linkOpen = md.renderer.rules.link_open ?? ((t, i, op, e, s) => s.renderToken(t, i, op));
  md.renderer.rules.link_open = (tokens, i, opts, env, self) => {
    const href = String(tokens[i].attrGet('href') ?? '');
    if (!/^[a-z]+:|^#/i.test(href)) tokens[i].attrSet('href', o.fileUrl(decodeURIComponent(href)));
    tokens[i].attrSet('rel', 'noopener noreferrer');
    tokens[i].attrSet('target', '_blank');
    return linkOpen(tokens, i, opts, env, self);
  };
  return md;
}

export function renderMarkdown(src: string, o: RenderOptions): string {
  let text = src.replace(/\r\n/g, '\n');
  const lines = text.split('\n');
  const footer = parseFooter(lines);
  let footerHtml = '';
  if (footer.startLine !== null) {
    text = lines.slice(0, footer.startLine).join('\n');
    if (o.footerTable && footer.fields.length) {
      footerHtml = `<table class="properties">${footer.fields.map(([k, v]) => `<tr><th>${esc(k)}</th><td>${esc(v)}</td></tr>`).join('')}</table>`;
    }
  }
  if (o.dropTitle) text = text.replace(/^#\s[^\n]*\n?/, '');
  return createRenderer(o).render(text) + footerHtml;
}

/** Post-process rendered HTML (math, mermaid) inside a container. */
export async function enhanceRendered(root: HTMLElement) {
  const math = root.querySelectorAll<HTMLElement>('.math, .math-block');
  if (math.length) {
    const [k] = await Promise.all([import('katex'), import('katex/dist/katex.min.css')]);
    const katex = k.default ?? k;
    math.forEach((el) => {
      try {
        katex.render(el.dataset.tex ?? '', el, { displayMode: el.classList.contains('math-block'), throwOnError: false });
      } catch {
        /* keep source */
      }
    });
  }
  const diagrams = root.querySelectorAll<HTMLElement>('.mermaid-src');
  if (diagrams.length) {
    const m = (await import('mermaid')).default;
    m.initialize({ startOnLoad: false, securityLevel: 'strict', theme: document.documentElement.dataset.theme === 'dark' ? 'dark' : 'neutral' });
    let n = 0;
    for (const el of diagrams) {
      try {
        const { svg } = await m.render(`lull-md-mermaid-${Date.now()}-${n++}`, el.dataset.code ?? '');
        el.innerHTML = svg;
        el.classList.add('mermaid');
      } catch {
        el.textContent = el.dataset.code ?? '';
      }
    }
  }
}
