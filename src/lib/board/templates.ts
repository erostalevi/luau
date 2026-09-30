// Built-in board and card templates (localized at render time).

import { t } from '$lib/i18n/index.svelte';

export interface BoardTemplate {
  id: string;
  kind: 'kanban' | 'files';
  lanes: string[];
}

export const BOARD_TEMPLATES: BoardTemplate[] = [
  { id: 'simple', kind: 'kanban', lanes: ['todo', 'doing', 'done'] },
  { id: 'product', kind: 'kanban', lanes: ['backlog', 'next', 'inProgress', 'review', 'done'] },
  { id: 'personal', kind: 'kanban', lanes: ['inbox', 'today', 'thisWeek', 'later', 'done'] },
  { id: 'content', kind: 'kanban', lanes: ['ideas', 'drafting', 'editing', 'scheduled', 'published'] },
  { id: 'bugs', kind: 'kanban', lanes: ['new', 'triaged', 'fixing', 'verifying', 'closed'] },
  { id: 'empty', kind: 'kanban', lanes: [] },
  { id: 'notes', kind: 'files', lanes: [] },
];

export function templateLanes(tpl: BoardTemplate): string[] {
  return tpl.lanes.map((l) => t(`templates.lanes.${l}`));
}

export interface CardTemplate {
  id: string;
  icon?: string;
  body: () => string;
}

export const CARD_TEMPLATES: CardTemplate[] = [
  { id: 'blank', body: () => `# \n` },
  {
    id: 'task',
    body: () => `# ${t('templates.cards.task.title')}\n\n${t('templates.cards.task.why')}\n\n- [ ] \n- [ ] \n`,
  },
  {
    id: 'bug',
    body: () =>
      `# ${t('templates.cards.bug.title')}\n\n## ${t('templates.cards.bug.steps')}\n\n1. \n2. \n\n## ${t('templates.cards.bug.expected')}\n\n\n## ${t('templates.cards.bug.actual')}\n\n\n#bug\n\n---\npriority: high\n`,
  },
  {
    id: 'feature',
    body: () =>
      `# ${t('templates.cards.feature.title')}\n\n## ${t('templates.cards.feature.problem')}\n\n\n## ${t('templates.cards.feature.proposal')}\n\n\n## ${t('templates.cards.feature.done')}\n\n- [ ] \n\n#feature\n`,
  },
  {
    id: 'meeting',
    body: () => {
      const d = new Date().toISOString().slice(0, 10);
      return `# ${t('templates.cards.meeting.title')} [${d}]\n\n## ${t('templates.cards.meeting.attendees')}\n\n- @\n\n## ${t('templates.cards.meeting.notes')}\n\n\n## ${t('templates.cards.meeting.actions')}\n\n- [ ] \n`;
    },
  },
  {
    id: 'decision',
    body: () =>
      `# ${t('templates.cards.decision.title')}\n\n> [!note] ${t('templates.cards.decision.status')}\n\n## ${t('templates.cards.decision.context')}\n\n\n## ${t('templates.cards.decision.options')}\n\n| ${t('templates.cards.decision.option')} | ${t('templates.cards.decision.pros')} | ${t('templates.cards.decision.cons')} |\n|---|---|---|\n|  |  |  |\n\n## ${t('templates.cards.decision.outcome')}\n\n`,
  },
  {
    id: 'weekly',
    body: () =>
      `# ${t('templates.cards.weekly.title')}\n\n## ${t('templates.cards.weekly.wins')}\n\n- \n\n## ${t('templates.cards.weekly.blockers')}\n\n- \n\n## ${t('templates.cards.weekly.next')}\n\n- [ ] \n`,
  },
];
