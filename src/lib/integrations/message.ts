// Pure helpers for the WriteGate confirmation text (unit-tested).

import type { Prepared } from './types';

export type Tr = (key: string, params?: Record<string, unknown>) => string;

const SERVICE_NAMES: Record<string, string> = { jira: 'Jira', trello: 'Trello', slack: 'Slack', local: 'Lull' };

export function serviceName(s: string): string {
  return SERVICE_NAMES[s] ?? s;
}

/** "Summary and Description" / "A, B y C" in the active locale. */
export function joinList(items: string[], locale = 'en'): string {
  if (items.length <= 1) return items[0] ?? '';
  try {
    return new Intl.ListFormat(locale, { style: 'long', type: 'conjunction' }).format(items);
  } catch {
    return `${items.slice(0, -1).join(', ')} & ${items[items.length - 1]}`;
  }
}

/** Translate a field name produced by Rust (`Summary`, `Status K-1`, …). */
export function fieldLabel(field: string, tr: Tr): string {
  const [head, ...rest] = field.split(' ');
  const key = `integrations.fields.${head.toLowerCase()}`;
  const v = tr(key);
  const label = v === key ? head : v;
  return rest.length ? `${label} ${rest.join(' ')}` : label;
}

/** Where the change lands: "Jira (acme.atlassian.net) for K-1" or the local card. */
export function changeTarget(p: Prepared, tr: Tr): string {
  if (p.direction === 'pull') return tr('integrations.gate.localTarget', { target: p.target, service: serviceName(p.service) });
  return tr('integrations.gate.remoteTarget', { service: serviceName(p.service), site: p.site, target: p.target });
}

/** The dialog sentence: "This will update Summary and Description on …". */
export function changeMessage(p: Prepared, locale = 'en', tr: Tr): string {
  const fields = [...new Set(p.fields.map((f) => fieldLabel(f.replace(/ .*$/, ''), tr)))];
  return tr('integrations.gate.message', { fields: joinList(fields, locale), target: changeTarget(p, tr) });
}

