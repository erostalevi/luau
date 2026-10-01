// Keep keyboard focus inside a modal surface (dialogs, first run, cheat sheet,
// quick input). Focuses `initial` (selector) or the node on mount, cycles Tab /
// Shift+Tab within it and gives focus back to where it was on close.

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function trapFocus(node: HTMLElement, opts: { initial?: string | null } = {}) {
  const before = document.activeElement as HTMLElement | null;
  queueMicrotask(() => {
    if (node.contains(document.activeElement)) return;
    const first = opts.initial === null ? null : node.querySelector<HTMLElement>(opts.initial ?? FOCUSABLE);
    (first ?? node).focus();
  });
  const onkey = (e: KeyboardEvent) => {
    if (e.key !== 'Tab') return;
    const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null || el === document.activeElement);
    if (!items.length) {
      e.preventDefault();
      node.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const cur = document.activeElement as HTMLElement | null;
    if (e.shiftKey && (cur === first || !node.contains(cur))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (cur === last || !node.contains(cur))) {
      e.preventDefault();
      first.focus();
    }
  };
  node.addEventListener('keydown', onkey);
  return {
    destroy() {
      node.removeEventListener('keydown', onkey);
      if (before && document.contains(before)) queueMicrotask(() => before.focus());
    },
  };
}
