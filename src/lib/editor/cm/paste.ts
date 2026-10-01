// Paste & drop: files/images become attachments next to the card; rich HTML
// becomes Markdown; a URL pasted over a selection becomes a link.

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { isTauri } from '$lib/backend/rpc';

export interface PasteOptions {
  /** Store a file as an attachment; returns the file name to reference. */
  upload(file: File): Promise<string | null>;
  richPaste(): boolean;
}

const IMG = /^image\//;

let turndownReady: Promise<(html: string) => string> | null = null;
function htmlToMarkdown(): Promise<(html: string) => string> {
  turndownReady ??= Promise.all([import('turndown'), import('turndown-plugin-gfm')]).then(([td, gfm]) => {
    const T = td.default;
    const service = new T({ headingStyle: 'atx', codeBlockStyle: 'fenced', bulletListMarker: '-', emDelimiter: '*', hr: '---' });
    const plugin = (gfm as unknown as { gfm: unknown }).gfm ?? (gfm as unknown as { default: { gfm: unknown } }).default?.gfm;
    if (plugin) service.use(plugin as never);
    service.remove(['script', 'style', 'meta', 'link', 'noscript', 'iframe', 'object']);
    return (html: string) => service.turndown(html).replace(/\n{3,}/g, '\n\n');
  });
  return turndownReady;
}

export function fileMarkdown(name: string, file: string, mime: string): string {
  const safe = encodeURI(file);
  if (IMG.test(mime) || /\.(png|jpe?g|gif|webp|svg|avif)$/i.test(name) || /\.pdf$/i.test(name) || /^(video|audio)\//.test(mime))
    return `![${name.replace(/[[\]|]/g, '')}](${safe})`;
  return `[${name.replace(/[[\]]/g, '')}](${safe})`;
}

async function insertFiles(view: EditorView, files: File[], pos: number, opts: PasteOptions) {
  const parts: string[] = [];
  for (const f of files) {
    const name = f.name || (IMG.test(f.type) ? `image.${f.type.split('/')[1] || 'png'}` : 'file');
    const stored = await opts.upload(f);
    if (stored) parts.push(fileMarkdown(name, stored, f.type));
  }
  if (!parts.length) return;
  // Images on the same line wrap naturally; separate with spaces.
  const text = parts.join(' ');
  view.dispatch({ changes: { from: pos, insert: text }, selection: { anchor: pos + text.length }, userEvent: 'input.paste' });
}

export function pasteAndDrop(opts: PasteOptions): Extension {
  return EditorView.domEventHandlers({
    paste(e, view) {
      const dt = e.clipboardData;
      if (!dt) return false;
      const files = [...dt.files];
      if (files.length) {
        e.preventDefault();
        void insertFiles(view, files, view.state.selection.main.from, opts);
        return true;
      }
      const text = dt.getData('text/plain');
      const sel = view.state.selection.main;
      if (!sel.empty && /^https?:\/\/\S+$/.test(text.trim())) {
        e.preventDefault();
        const label = view.state.sliceDoc(sel.from, sel.to);
        view.dispatch({ changes: { from: sel.from, to: sel.to, insert: `[${label}](${text.trim()})` }, userEvent: 'input.paste' });
        return true;
      }
      const html = dt.getData('text/html');
      // Plain text inside code blocks, and for copies from code editors /
      // terminals (only <pre>/monospace markup): turndown would escape * _ #
      // and drop indentation.
      const codey =
        isInCode(view) ||
        /^\s*(<meta[^>]*>\s*)?<(pre|code)\b/i.test(html.replace(/<!--[\s\S]*?-->/g, '')) ||
        /font-family:\s*[^;"]*(mono|menlo|consolas|courier)/i.test(html);
      if (html && opts.richPaste() && !html.includes('data-luau-plain') && !codey) {
        e.preventDefault();
        void htmlToMarkdown().then((conv) => {
          const md = conv(html).trim();
          const s = view.state.selection.main;
          view.dispatch({
            changes: { from: s.from, to: s.to, insert: md || text },
            selection: { anchor: s.from + (md || text).length },
            userEvent: 'input.paste',
          });
        });
        return true;
      }
      return false;
    },
    drop(e, view) {
      const files = [...(e.dataTransfer?.files ?? [])];
      if (!files.length) return false;
      e.preventDefault();
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? view.state.selection.main.head;
      void insertFiles(view, files, pos, opts);
      return true;
    },
    dragover(e) {
      if (e.dataTransfer?.types.includes('Files')) {
        e.preventDefault();
        return true;
      }
      return false;
    },
  });
}

function isInCode(view: EditorView) {
  for (
    let n: ReturnType<ReturnType<typeof syntaxTree>['resolveInner']> | null = syntaxTree(view.state).resolveInner(view.state.selection.main.head, -1);
    n;
    n = n.parent
  )
    if (/FencedCode|CodeBlock|InlineCode|CodeText/.test(n.name)) return true;
  return false;
}

/** Paste clipboard as plain text (⇧⌘V). Uses the native clipboard in the app
 *  (no WebKit paste bubble), the web API in the browser build. */
export async function pastePlain(view: EditorView) {
  const text = isTauri
    ? await import('@tauri-apps/plugin-clipboard-manager').then((m) => m.readText()).catch(() => '')
    : await navigator.clipboard.readText().catch(() => '');
  if (!text) return;
  const s = view.state.selection.main;
  view.dispatch({ changes: { from: s.from, to: s.to, insert: text }, selection: { anchor: s.from + text.length }, userEvent: 'input.paste' });
}
