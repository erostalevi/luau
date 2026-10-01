// Grouping of the integrations panel result list by remote project (Jira) or
// board (Trello). Pure; the panel renders one collapsible group per project.

import type { ProviderKind, RemoteIssue } from './types';

export interface IssueGroup {
  /** Stable id: Jira project key or Trello board id ('' = unknown). */
  id: string;
  /** Header text (project key / board name). */
  label: string;
  issues: RemoteIssue[];
}

const JIRA_KEY = /^([A-Za-z][A-Za-z0-9_]*)-(\d+)$/;

/** Project of a result row: the provider's `project` field, else (Jira) the key prefix. */
export function projectOf(i: Pick<RemoteIssue, 'key' | 'project'>, provider?: ProviderKind): string {
  if (i.project) return i.project;
  if (provider === 'trello') return '';
  return JIRA_KEY.exec(i.key)?.[1] ?? '';
}

/** Key without its project prefix: `FPDEV-1398` in `FPDEV` → `1398`.
 *  Anything that does not carry that exact prefix is returned unchanged. */
export function shortKey(key: string, project: string): string {
  if (!project) return key;
  const p = `${project}-`;
  if (key.length > p.length && key.slice(0, p.length).toUpperCase() === p.toUpperCase()) return key.slice(p.length);
  return key;
}

/** Group `issues` by project, keeping the result order: groups appear in the
 *  order of their first issue, issues keep their relative order. */
export function groupIssues(issues: RemoteIssue[], provider?: ProviderKind, names: Record<string, string> = {}): IssueGroup[] {
  const byId = new Map<string, IssueGroup>();
  for (const i of issues) {
    const id = projectOf(i, provider);
    let g = byId.get(id);
    if (!g) {
      g = { id, label: names[id] || id, issues: [] };
      byId.set(id, g);
    }
    g.issues.push(i);
  }
  return [...byId.values()];
}

/** Persisted collapse key: account + search mode + query (the one that produced
 *  the results) + project. */
export function groupStateKey(account: string, mode: string, query: string, project: string): string {
  return JSON.stringify([account, mode, query.trim(), project]);
}

/** Keys of the rows currently shown (collapsed groups hidden), in display order. */
export function visibleKeys(groups: IssueGroup[], isCollapsed: (g: IssueGroup) => boolean): string[] {
  return groups.flatMap((g) => (isCollapsed(g) ? [] : g.issues.map((i) => i.key)));
}

/** Set or clear one collapsed flag; only collapsed entries are stored, oldest
 *  dropped beyond `cap` so the map cannot grow without bound. */
export function setCollapsed(map: Record<string, true>, key: string, collapsed: boolean, cap = 300): Record<string, true> {
  const next = { ...map };
  delete next[key];
  if (collapsed) next[key] = true;
  const keys = Object.keys(next);
  for (const k of keys.slice(0, Math.max(0, keys.length - cap))) delete next[k];
  return next;
}
