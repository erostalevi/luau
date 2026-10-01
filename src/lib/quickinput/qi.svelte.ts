// VS Code-style QuickInput: quickPick / inputBox with multi-step flows.
//
//   const lane = await quickPick(lanes, { title: 'Move card', step: 2, totalSteps: 3 });
//   if (lane === BACK) …; if (lane === undefined) return; // cancelled
//
// `steps()` runs a sequence with automatic back navigation.

import type { Component } from 'svelte';
import { ctx } from '$lib/commands/context.svelte';

export const BACK: unique symbol = Symbol('back');
export type Back = typeof BACK;

export interface QuickItem<T = unknown> {
  label: string;
  value?: T;
  description?: string;
  detail?: string;
  icon?: Component<any>;
  iconColor?: string;
  keybinding?: string | null;
  kind?: 'separator';
  picked?: boolean;
  alwaysShow?: boolean;
  /** Pre-computed match positions from a custom filter. */
  highlights?: number[];
}

export interface PickOptions<T> {
  title?: string;
  placeholder?: string;
  step?: number;
  totalSteps?: number;
  value?: string;
  canPickMany?: boolean;
  matchOnDescription?: boolean;
  matchOnDetail?: boolean;
  /** Let the user submit the typed text when nothing matches. */
  allowCustom?: (text: string) => QuickItem<T> | null;
  /** Dynamic items (items ignored when set); may filter itself. */
  onValue?: (text: string) => QuickItem<T>[] | Promise<QuickItem<T>[]>;
  /** When onValue filters itself, skip fuzzy filtering. */
  selfFiltered?: boolean;
  /** Keep the active item on the first match (default) or a given index. */
  activeIndex?: number;
  /** Called on active item change (preview). */
  onActive?: (item: QuickItem<T> | undefined) => void;
  /** Prefix routing for the palette: typing a new prefix switches mode. */
  prefixRouter?: (text: string) => boolean;
  /** Select the pre-filled value on open (default). `false` puts the caret at the end. */
  selectAll?: boolean;
}

export interface InputOptions {
  title?: string;
  placeholder?: string;
  prompt?: string;
  value?: string;
  step?: number;
  totalSteps?: number;
  password?: boolean;
  /** Return an error message (string) or null when valid. */
  validate?: (v: string) => string | null | Promise<string | null>;
  selectAll?: boolean;
}

export interface QIState {
  mode: 'pick' | 'input';
  title?: string;
  placeholder?: string;
  prompt?: string;
  step?: number;
  totalSteps?: number;
  items: QuickItem<any>[];
  value: string;
  canPickMany: boolean;
  selected: Set<number>;
  validation: string | null;
  busy: boolean;
  password: boolean;
  opts: PickOptions<any> | InputOptions;
  resolve: (v: any) => void;
  gen: number;
}

export const qi = $state<{ st: QIState | null }>({ st: null });

let prevFocus: HTMLElement | null = null;

function open(st: Omit<QIState, 'gen'>) {
  if (!qi.st) prevFocus = document.activeElement as HTMLElement | null;
  qi.st?.resolve(undefined);
  qi.st = { ...st, gen: (qi.st?.gen ?? 0) + 1 };
  ctx.paletteOpen = true;
}

export function closeQuickInput(result?: unknown) {
  const st = qi.st;
  qi.st = null;
  ctx.paletteOpen = false;
  st?.resolve(result);
  if (result === undefined) prevFocus?.focus?.();
}

export function quickPick<T>(items: QuickItem<T>[], opts: PickOptions<T> = {}): Promise<T | T[] | undefined | Back> {
  return new Promise((resolve) => {
    open({
      mode: 'pick',
      title: opts.title,
      placeholder: opts.placeholder,
      step: opts.step,
      totalSteps: opts.totalSteps,
      items,
      value: opts.value ?? '',
      canPickMany: !!opts.canPickMany,
      selected: new Set(items.map((it, i) => (it.picked ? i : -1)).filter((i) => i >= 0)),
      validation: null,
      busy: false,
      password: false,
      opts,
      resolve,
    });
  });
}

export function inputBox(opts: InputOptions = {}): Promise<string | undefined | Back> {
  return new Promise((resolve) => {
    open({
      mode: 'input',
      title: opts.title,
      placeholder: opts.placeholder,
      prompt: opts.prompt,
      step: opts.step,
      totalSteps: opts.totalSteps,
      items: [],
      value: opts.value ?? '',
      canPickMany: false,
      selected: new Set(),
      validation: null,
      busy: false,
      password: !!opts.password,
      opts,
      resolve,
    });
  });
}

/** Single-pick convenience that narrows the type. */
export async function pickOne<T>(items: QuickItem<T>[], opts: PickOptions<T> = {}): Promise<T | undefined | Back> {
  const r = await quickPick(items, { ...opts, canPickMany: false });
  return r as T | undefined | Back;
}

/**
 * Run steps with back support. Each step receives the results so far and
 * returns a value, `undefined` (cancel) or `BACK`.
 */
export async function steps<R extends unknown[]>(fns: ((results: any[]) => Promise<unknown>)[]): Promise<R | undefined> {
  const results: unknown[] = [];
  let i = 0;
  while (i < fns.length) {
    const r = await fns[i](results);
    if (r === undefined) return undefined;
    if (r === BACK) {
      if (i === 0) return undefined;
      i--;
      results.pop();
      continue;
    }
    results[i] = r;
    i++;
  }
  return results as R;
}
