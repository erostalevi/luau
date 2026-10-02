// "Task from Slack": your newest @mention becomes one or more cards (the AI
// drafts them, you confirm), each linking back to the Slack message.

import { rpc } from '$lib/backend/rpc';
import { inputBox } from '$lib/quickinput/qi.svelte';
import { settings } from '$lib/settings/store.svelte';
import { toast, dismiss } from '$lib/state/toasts.svelte';
import { t } from '$lib/i18n/index.svelte';

export interface Mention {
  channel: string;
  channelName: string;
  ts: string;
  author: string;
  text: string;
  permalink: string | null;
}

const MEMBER_ID = /^[UW][A-Z0-9]{6,20}$/;

/** Text handed to the card drafter (pure). */
export function mentionText(m: Mention): string {
  const where = m.channelName ? ` in #${m.channelName}` : '';
  return `${m.text.trim()}\n\n— @${m.author}${where}`;
}

export function mentionFooter(m: Mention): string {
  return m.permalink ? `${t('slackTask.source')}: [${t('slackTask.message')}](${m.permalink})` : `${t('slackTask.source')}: Slack`;
}

async function askMemberId(): Promise<string | null> {
  const v = await inputBox({
    title: t('slackTask.memberTitle'),
    prompt: t('slackTask.memberPrompt'),
    placeholder: 'U012ABCDEF',
    validate: (s) => (MEMBER_ID.test(s.trim()) ? null : t('slackTask.memberInvalid')),
  });
  if (typeof v !== 'string' || !MEMBER_ID.test(v.trim())) return null;
  settings.set('integrations.slackMemberId', v.trim());
  return v.trim();
}

export async function taskFromSlack() {
  let member = String(settings.get('integrations.slackMemberId') ?? '') || null;
  const tid = toast.info(t('slackTask.reading'), { timeout: 0 });
  let m: Mention | null = null;
  try {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        m = await rpc<Mention | null>('slack.latestMention', { member });
        break;
      } catch (e) {
        if (!(e as Error).message.includes('need_member_id') || attempt > 0) throw e;
        dismiss(tid);
        member = await askMemberId();
        if (!member) return;
      }
    }
  } catch (e) {
    const msg = (e as Error).message;
    toast.error(
      /slack account/i.test(msg)
        ? t('slackTask.notConnected')
        : /missing_scope|not_in_channel/.test(msg)
          ? t('slackTask.scopes')
          : t('slackTask.failed', { message: msg }),
    );
    return;
  } finally {
    dismiss(tid);
  }
  if (!m) {
    toast.info(t('slackTask.none'));
    return;
  }
  const { fromText } = await import('$lib/clipboard/clipboard.commands');
  await fromText(mentionText(m), { preset: 'ai', footer: mentionFooter(m), title: t('commands.ai.slackTask') });
}
