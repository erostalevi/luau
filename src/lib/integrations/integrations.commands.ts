// Integration commands + contributions (card-face strip, editor header,
// card actions, drag & drop handlers, settings).

import {
  ArrowDownToLine,
  ArrowUpFromLine,
  ExternalLink,
  GitPullRequestArrow,
  MessageSquare,
  Plug,
  PlusCircle,
  Send,
  UserPlus,
  Unlink,
  RefreshCw,
  Copy,
  ListPlus,
} from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';
import { contribute } from '$lib/contributions/registry.svelte';
import { settings } from '$lib/settings/store.svelte';
import { onExternalDrop } from '$lib/board/dnd.svelte';
import { t } from '$lib/i18n/index.svelte';
import RemoteStrip from './RemoteStrip.svelte';
import RemoteHeader from './RemoteHeader.svelte';
import { initIntegrationState } from './state.svelte';
import * as A from './actions';
import type { IssueDragPayload, IssuesDragPayload } from './types';

export const commands: Command[] = [
  { id: 'integrations.connectJira', title: 'commands.integrations.connectJira', category: 'jira', icon: Plug, run: () => A.connectJira() },
  { id: 'integrations.connectTrello', title: 'commands.integrations.connectTrello', category: 'trello', icon: Plug, run: () => A.connectTrello() },
  { id: 'integrations.connectSlack', title: 'commands.integrations.connectSlack', category: 'slack', icon: Plug, run: () => A.connectSlack() },
  { id: 'integrations.mirrorBoard', title: 'commands.integrations.mirrorBoard', category: 'integrations', icon: Copy, run: () => A.mirrorBoard() },
  { id: 'remote.pull', title: 'commands.remote.pull', category: 'integrations', icon: ArrowDownToLine, run: () => A.pull() },
  { id: 'remote.push', title: 'commands.remote.push', category: 'integrations', icon: ArrowUpFromLine, when: 'cardIsRemote', run: () => A.push() },
  {
    id: 'remote.createIssue',
    title: 'commands.remote.createIssue',
    category: 'integrations',
    icon: PlusCircle,
    when: '!cardIsRemote && !boardReadOnly',
    run: () => A.createIssue(),
  },
  {
    id: 'remote.openInBrowser',
    title: 'commands.remote.openInBrowser',
    category: 'integrations',
    icon: ExternalLink,
    when: 'cardIsRemote',
    run: () => A.openInBrowser(),
  },
  { id: 'remote.comment', title: 'commands.remote.comment', category: 'integrations', icon: MessageSquare, when: 'cardIsRemote', run: () => A.comment() },
  {
    id: 'remote.transition',
    title: 'commands.remote.transition',
    category: 'integrations',
    icon: GitPullRequestArrow,
    when: 'cardIsRemote',
    run: () => A.transition(),
  },
  { id: 'remote.assign', title: 'commands.remote.assign', category: 'integrations', icon: UserPlus, when: 'cardIsRemote', run: () => A.assign() },
  { id: 'remote.unlink', title: 'commands.remote.unlink', category: 'integrations', icon: Unlink, when: 'cardIsRemote', run: () => A.unlink() },
  { id: 'remote.refresh', title: 'commands.remote.refresh', category: 'integrations', icon: RefreshCw, run: () => A.refreshLinks() },
  { id: 'remote.actions', title: 'commands.remote.actions', category: 'integrations', when: 'cardIsRemote', hidden: true, run: () => A.remoteActions() },
  { id: 'remote.toggleAllowPush', title: 'commands.remote.toggleAllowPush', category: 'integrations', run: () => A.toggleAllow('push') },
  { id: 'remote.toggleAllowPull', title: 'commands.remote.toggleAllowPull', category: 'integrations', run: () => A.toggleAllow('pull') },
  { id: 'slack.post', title: 'commands.slack.post', category: 'slack', icon: Send, run: () => A.postToSlack() },
  {
    id: 'integrations.addSelectedTo',
    title: 'commands.integrations.addSelectedTo',
    category: 'integrations',
    icon: ListPlus,
    run: () => A.addSelectedTo(),
  },
];

export function init() {
  settings.register([
    { key: 'integrations.allowPush', type: 'boolean', default: false, category: 'integrations' },
    { key: 'integrations.allowPull', type: 'boolean', default: true, category: 'integrations' },
  ]);
  initIntegrationState();

  contribute('cardFace', { id: 'remote.strip', component: RemoteStrip, when: (c) => !!c.remote, placement: 'external', source: 'integrations' });
  contribute('editorHeader', { id: 'remote.header', component: RemoteHeader, when: (c) => !!c.remote, source: 'integrations' });
  contribute('cardActions', {
    id: 'remote.transition',
    label: () => t('commands.remote.transition'),
    icon: GitPullRequestArrow,
    when: (c) => !!c.remote,
    run: (c) => A.transition(c),
    source: 'integrations',
  });
  contribute('cardActions', {
    id: 'remote.comment',
    label: () => t('commands.remote.comment'),
    icon: MessageSquare,
    when: (c) => !!c.remote,
    run: (c) => A.comment(c),
    source: 'integrations',
  });
  contribute('cardActions', {
    id: 'remote.openInBrowser',
    label: () => t('commands.remote.openInBrowser'),
    icon: ExternalLink,
    when: (c) => !!c.remote,
    run: (c) => A.openInBrowser(c),
    source: 'integrations',
  });
  contribute('cardActions', {
    id: 'remote.createIssue',
    label: () => t('commands.remote.createIssue'),
    icon: PlusCircle,
    when: (c) => !c.remote,
    run: (c) => A.createIssue(c),
    source: 'integrations',
  });

  // Issue rows dragged from the panel → linked copy at the drop position.
  onExternalDrop('remoteIssue', (src, tg) => {
    const p = src.payload as IssueDragPayload;
    return A.linkIssue(p.account, p.key, tg.boardId, tg.parent, tg.before);
  });
  // Several selected rows dragged at once → one batch of linked copies.
  onExternalDrop('remoteIssues', (src, tg) => {
    const p = src.payload as IssuesDragPayload;
    return A.linkMany(p.account, p.keys, tg.boardId, tg.parent, tg.before);
  });
  // Cards dragged out of a mirror board → linked copies.
  onExternalDrop('copyFromMirror', (src, tg) => A.copyFromMirror(src.boardId!, src.ids ?? [], tg.boardId, tg.parent, tg.before));
}
