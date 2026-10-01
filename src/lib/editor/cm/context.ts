// Editor context facet: gives widgets access to app services without imports
// of app state inside CodeMirror code.

import { Facet } from '@codemirror/state';
import type { Attachment } from '$lib/backend/types';

export interface CodeResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  images: string[];
  ms: number;
}

export interface LuauEditorContext {
  boardId: string;
  cardId: string;
  /** URL for a file referenced relative to the card (or remote URL). */
  fileUrl(ref: string, width?: number): string;
  attachment(ref: string): Attachment | undefined;
  titleOf(id: string): string;
  isMissing(id: string): boolean;
  openCard(id: string, heading?: string | null): void;
  openExternal(url: string): void;
  openFile(ref: string): void;
  /** Render another card's content (embeds) into `el`. Returns a cleanup. */
  renderEmbed(id: string, heading: string | null, el: HTMLElement): () => void;
  runCode(lang: string, code: string): Promise<CodeResult>;
  cachedOutput(lang: string, code: string): CodeResult | null;
  linkPreview(url: string): Promise<{ title: string; description: string; image: string | null; site: string } | null>;
  tagStyle(tag: string): string;
  formatDate(iso: string): string;
  editFooter(): void;
  t(key: string, params?: Record<string, unknown>): string;
}

export const luauContext = Facet.define<LuauEditorContext, LuauEditorContext | null>({
  combine: (values) => values[values.length - 1] ?? null,
});
