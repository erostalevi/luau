// Initial text selection of the quick input box when it opens.

/**
 * Selection range for a freshly opened quick input.
 * - `selectAll` (default) selects the whole pre-filled value, so typing replaces it.
 * - `selectAll: false` places the caret at the end with nothing selected, so a palette
 *   mode prefix such as `>` is kept and typing filters inside that mode.
 */
export function initialSelection(value: string, selectAll: boolean | undefined): [start: number, end: number] {
  const end = value.length;
  return selectAll === false ? [end, end] : [0, end];
}
