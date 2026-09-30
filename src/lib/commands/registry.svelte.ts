// Command registry: every user action is a command. The palette, keybindings,
// menus, context menus and (later) extensions all go through here.

import type { Component } from 'svelte';
import { t } from '$lib/i18n/index.svelte';
import { evaluateWhen } from './when';
import { ctxGet } from './context.svelte';

export interface Command {
  id: string;
  /** i18n key (or literal when `literal` is set). */
  title: string;
  category?: string;
  icon?: Component<any>;
  when?: string;
  /** Hidden from the command palette (still bindable). */
  hidden?: boolean;
  literal?: boolean;
  source?: string;
  run: (args?: any) => unknown | Promise<unknown>;
}

const commands = new Map<string, Command>();
export const commandsVersion = $state({ v: 0 });
const recent: string[] = [];

export function registerCommand(cmd: Command): () => void {
  commands.set(cmd.id, { source: 'core', ...cmd });
  commandsVersion.v++;
  return () => {
    commands.delete(cmd.id);
    commandsVersion.v++;
  };
}

export function registerCommands(list: Command[]): () => void {
  const offs = list.map(registerCommand);
  return () => offs.forEach((o) => o());
}

export function getCommand(id: string): Command | undefined {
  return commands.get(id);
}

export function allCommands(): Command[] {
  void commandsVersion.v;
  return [...commands.values()];
}

export function commandTitle(c: Command): string {
  return c.literal ? c.title : t(c.title);
}

export function commandCategory(c: Command): string {
  return c.category ? t(`commands.categories.${c.category}`) : '';
}

export function isEnabled(c: Command): boolean {
  return evaluateWhen(c.when, ctxGet);
}

export function recentCommands(): string[] {
  return recent;
}

export async function runCommand(id: string, args?: unknown): Promise<unknown> {
  const c = commands.get(id);
  if (!c) {
    console.warn('unknown command', id);
    return;
  }
  if (!isEnabled(c)) return;
  const i = recent.indexOf(id);
  if (i >= 0) recent.splice(i, 1);
  recent.unshift(id);
  recent.length = Math.min(recent.length, 12);
  try {
    return await c.run(args);
  } catch (e) {
    console.error(`command ${id} failed`, e);
    const { toast } = await import('$lib/state/toasts.svelte');
    toast.error(e instanceof Error ? e.message : String(e));
  }
}
