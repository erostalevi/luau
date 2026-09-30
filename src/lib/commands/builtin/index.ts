import { registerCommands, type Command } from '../registry.svelte';
import { appCommands } from './app';
import { workspaceCommands } from './workspace';
import { boardCommands } from './boards';
import { boardCardCommands } from '$lib/board/commands';
import { editorCommands } from '$lib/editor/editorCommands';

/** Feature modules anywhere under src/lib named `*.commands.ts` export `commands`. */
const featureModules = import.meta.glob<{ commands: Command[]; init?: () => void }>('/src/lib/**/*.commands.ts', { eager: true });

let done = false;

export function registerBuiltinCommands() {
  if (done) return;
  done = true;
  registerCommands([...appCommands, ...workspaceCommands, ...boardCommands, ...boardCardCommands, ...editorCommands]);
  for (const mod of Object.values(featureModules)) {
    if (mod.commands) registerCommands(mod.commands);
    mod.init?.();
  }
}
