// AI commands: Change with AI… (more join as they land).

import { Wand2 } from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';

export const commands: Command[] = [
  {
    id: 'ai.change',
    title: 'commands.ai.change',
    category: 'ai',
    icon: Wand2,
    when: 'editorOpen',
    run: async (args?: { instruction?: string }) => (await import('./change.svelte')).changeWithAi(args?.instruction),
  },
];
