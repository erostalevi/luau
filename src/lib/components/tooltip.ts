// `use:tip={'Label'}` or `use:tip={{ text, command }}` — a calm delayed tooltip.

import { formatKey } from '$lib/keybindings/keys';
import { primaryKey } from '$lib/keybindings/resolver.svelte';

type TipArg = string | { text: string; command?: string; keys?: string; placement?: 'top' | 'bottom' | 'right' | 'left' } | null | undefined;

let el: HTMLDivElement | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let lastHide = 0;

function ensure(): HTMLDivElement {
  if (el) return el;
  el = document.createElement('div');
  el.className = 'lull-tip';
  document.body.appendChild(el);
  return el;
}

function show(target: HTMLElement, arg: Exclude<TipArg, null | undefined>) {
  const o = typeof arg === 'string' ? { text: arg } : arg;
  if (!o.text) return;
  const tip = ensure();
  const key = o.keys ?? (o.command ? primaryKey(o.command) : null);
  tip.innerHTML = '';
  const span = document.createElement('span');
  span.textContent = o.text;
  tip.appendChild(span);
  if (key) {
    const k = document.createElement('span');
    k.className = 'lull-tip-k';
    k.textContent = formatKey(key).join(' ');
    tip.appendChild(k);
  }
  tip.style.opacity = '0';
  tip.style.display = 'flex';
  const r = target.getBoundingClientRect();
  const tr = tip.getBoundingClientRect();
  const placement = typeof arg === 'object' && arg.placement ? arg.placement : 'bottom';
  let x = r.left + r.width / 2 - tr.width / 2;
  let y = placement === 'top' ? r.top - tr.height - 8 : r.bottom + 8;
  if (placement === 'right') {
    x = r.right + 8;
    y = r.top + r.height / 2 - tr.height / 2;
  }
  if (placement === 'left') {
    x = r.left - tr.width - 8;
    y = r.top + r.height / 2 - tr.height / 2;
  }
  x = Math.max(6, Math.min(window.innerWidth - tr.width - 6, x));
  if (y + tr.height > window.innerHeight - 6) y = r.top - tr.height - 8;
  tip.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
  tip.style.opacity = '1';
}

function hide() {
  if (timer) clearTimeout(timer);
  timer = null;
  if (el) {
    el.style.opacity = '0';
    el.style.display = 'none';
  }
  lastHide = Date.now();
}

export function tip(node: HTMLElement, arg: TipArg) {
  let current = arg;
  const enter = () => {
    if (!current) return;
    const warm = Date.now() - lastHide < 400;
    timer = setTimeout(() => current && show(node, current), warm ? 60 : 550);
  };
  node.addEventListener('pointerenter', enter);
  node.addEventListener('pointerleave', hide);
  node.addEventListener('pointerdown', hide);
  return {
    update(a: TipArg) {
      current = a;
    },
    destroy() {
      hide();
      node.removeEventListener('pointerenter', enter);
      node.removeEventListener('pointerleave', hide);
      node.removeEventListener('pointerdown', hide);
    },
  };
}
