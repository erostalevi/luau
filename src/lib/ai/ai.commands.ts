// AI commands: Change with AI…, Quick summary…, AI… (assistant).

import { Wand2, MessageCircleQuestion, Sparkles } from '@lucide/svelte';
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
  {
    id: 'ai.ask',
    title: 'commands.ai.ask',
    category: 'ai',
    icon: MessageCircleQuestion,
    run: async (args?: { question?: string }) => (await import('./assistant.svelte')).openAssistant('ask', args?.question),
  },
  {
    id: 'ai.assistant',
    title: 'commands.ai.assistant',
    category: 'ai',
    icon: Sparkles,
    run: async (args?: { message?: string }) => (await import('./assistant.svelte')).openAssistant('agent', args?.message),
  },
];
