// `pickColor()` opens a small modal color picker and resolves with the hex.

export const colorDialog = $state<{ open: boolean; title: string; value: string; live?: (hex: string) => void; resolve: ((v: string | null) => void) | null }>({
  open: false,
  title: '',
  value: '#ef8a7c',
  resolve: null,
});

export function pickColor(title: string, initial: string, live?: (hex: string) => void): Promise<string | null> {
  colorDialog.resolve?.(null);
  return new Promise((resolve) => {
    colorDialog.open = true;
    colorDialog.title = title;
    colorDialog.value = initial;
    colorDialog.live = live;
    colorDialog.resolve = resolve;
  });
}

export function closeColorDialog(ok: boolean) {
  const r = colorDialog.resolve;
  colorDialog.open = false;
  colorDialog.resolve = null;
  r?.(ok ? colorDialog.value : null);
}
