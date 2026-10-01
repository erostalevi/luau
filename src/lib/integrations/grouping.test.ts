import { describe, expect, it } from 'vitest';
import { groupIssues, groupStateKey, projectOf, setCollapsed, shortKey, visibleKeys } from './grouping';
import type { RemoteIssue } from './types';

const issue = (key: string, project?: string | null): RemoteIssue => ({
  key,
  id: key,
  url: '',
  summary: key,
  descriptionMd: '',
  status: '',
  statusId: '',
  statusCategory: 'todo',
  labels: [],
  subtasks: [],
  attachments: [],
  project,
});

describe('result grouping', () => {
  it('project comes from the issue, else the Jira key prefix', () => {
    expect(projectOf(issue('FPDEV-1398', 'FPDEV'))).toBe('FPDEV');
    expect(projectOf(issue('FPDEV-1398'))).toBe('FPDEV');
    expect(projectOf(issue('ABC_2-7'), 'jiraServer')).toBe('ABC_2');
    expect(projectOf(issue('weird key'))).toBe('');
    // Trello keys carry no project; the board id comes from the issue only.
    expect(projectOf(issue('aB3dE', 'board1'), 'trello')).toBe('board1');
    expect(projectOf(issue('X-1'), 'trello')).toBe('');
  });

  it('strips only the exact project prefix', () => {
    expect(shortKey('FPDEV-1398', 'FPDEV')).toBe('1398');
    expect(shortKey('fpdev-12', 'FPDEV')).toBe('12');
    expect(shortKey('FPDEVX-1', 'FPDEV')).toBe('FPDEVX-1');
    expect(shortKey('OTHER-5', 'FPDEV')).toBe('OTHER-5');
    expect(shortKey('FPDEV-', 'FPDEV')).toBe('FPDEV-');
    expect(shortKey('aB3dE', 'board1')).toBe('aB3dE');
    expect(shortKey('X-1', '')).toBe('X-1');
  });

  it('groups by first appearance, keeping the result order inside groups', () => {
    const g = groupIssues([issue('B-1'), issue('A-9'), issue('B-2'), issue('odd'), issue('A-3')]);
    expect(g.map((x) => x.id)).toEqual(['B', 'A', '']);
    expect(g[0].issues.map((i) => i.key)).toEqual(['B-1', 'B-2']);
    expect(g[1].issues.map((i) => i.key)).toEqual(['A-9', 'A-3']);
    expect(g[2].label).toBe('');
  });

  it('a single project still makes one group; names label Trello boards', () => {
    expect(groupIssues([issue('A-1'), issue('A-2')])).toHaveLength(1);
    expect(groupIssues([]).length).toBe(0);
    const t = groupIssues([issue('c1', 'b1'), issue('c2', 'b2')], 'trello', { b1: 'Roadmap' });
    expect(t.map((x) => x.label)).toEqual(['Roadmap', 'b2']);
  });

  it('visible keys skip collapsed groups', () => {
    const g = groupIssues([issue('A-1'), issue('B-1'), issue('A-2')]);
    expect(visibleKeys(g, () => false)).toEqual(['A-1', 'A-2', 'B-1']);
    expect(visibleKeys(g, (x) => x.id === 'A')).toEqual(['B-1']);
  });

  it('collapse state is keyed per account + query + project and capped', () => {
    const k = groupStateKey('acc', 'jql', ' assignee = me ', 'A');
    expect(k).toBe(groupStateKey('acc', 'jql', 'assignee = me', 'A'));
    expect(k).not.toBe(groupStateKey('acc', 'text', 'assignee = me', 'A'));
    expect(k).not.toBe(groupStateKey('acc', 'jql', 'assignee = me', 'B'));
    let m = setCollapsed({}, k, true);
    expect(m).toEqual({ [k]: true });
    m = setCollapsed(m, k, false);
    expect(m).toEqual({});
    m = setCollapsed(setCollapsed(setCollapsed({}, 'a', true, 2), 'b', true, 2), 'c', true, 2);
    expect(Object.keys(m)).toEqual(['b', 'c']);
  });
});
