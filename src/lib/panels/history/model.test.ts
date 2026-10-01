import { describe, expect, it } from 'vitest';
import type { JournalEntry } from '$lib/backend/types';
import { daysLeft, describeEntry, entryCardId, entryTitles, filterEntries, fold, groupByDay, hasVersions, kindGroup, rangeFrom, sourceOf } from './model';

const E = (over: Partial<JournalEntry>): JournalEntry => ({
  ts: '2026-09-30T10:00:00.000Z',
  board: 'b1',
  kind: 'move',
  origin: 'you',
  label: 'x',
  ids: [],
  ...over,
});
const item = (id: string, title: string, lane?: string) => ({
  id,
  title,
  lane: lane ?? null,
  parent: null,
});

describe('describeEntry', () => {
  it('describes a move between lanes with human names', () => {
    const e = E({
      kind: 'move',
      ids: ['cabc123'],
      details: {
        before: {
          items: [item('cabc123', 'Fix login', 'Doing')],
          to: { lane: 'Done' },
        },
        after: {
          items: [item('cabc123', 'Fix login', 'Done')],
          to: { lane: 'Done' },
        },
      },
    });
    const d = describeEntry(e);
    expect(d.key).toBe('movedFromTo');
    expect(d.params).toMatchObject({
      title: 'Fix login',
      from: 'Doing',
      to: 'Done',
    });
    expect(d.group).toBe('moved');
  });

  it('describes multi-card moves and nesting', () => {
    const many = E({
      details: {
        before: {
          items: [item('c1', 'A', 'X'), item('c2', 'B', 'X')],
          to: { lane: 'Y' },
        },
        after: { items: [], to: { lane: 'Y' } },
      },
    });
    expect(describeEntry(many)).toMatchObject({
      key: 'movedManyTo',
      params: { count: 2, to: 'Y' },
    });
    const nest = E({
      details: { before: { items: [item('c1', 'A')], to: { card: 'Parent' } } },
    });
    expect(describeEntry(nest)).toMatchObject({
      key: 'nested',
      params: { target: 'Parent' },
    });
  });

  it('describes edits with char deltas', () => {
    const e = E({
      kind: 'edit',
      ids: ['c1'],
      details: { title: 'Notes', chars: 42 },
      before: 'aa',
      after: 'bb',
    });
    expect(describeEntry(e)).toMatchObject({
      key: 'edited',
      params: { title: 'Notes' },
      chars: 42,
    });
    expect(hasVersions(e)).toBe(true);
  });

  it('describes trash, restore and purge', () => {
    expect(
      describeEntry(
        E({
          kind: 'trash',
          details: { before: { items: [item('c1', 'Old')] } },
        }),
      ).key,
    ).toBe('deleted');
    expect(
      describeEntry(
        E({
          kind: 'trash',
          details: { before: { items: [], lanes: ['Ideas'] } },
        }),
      ),
    ).toMatchObject({ key: 'laneDeleted', params: { lane: 'Ideas' } });
    expect(
      describeEntry(
        E({
          kind: 'restore',
          ids: ['c1'],
          details: { before: null, after: null },
        }),
      ).key,
    ).toBe('restoredUntitled');
    // Titles resolved from the open board when details carry none.
    expect(describeEntry(E({ kind: 'restore', ids: ['c1'] }), undefined, (id) => (id === 'c1' ? 'Back again' : null))).toMatchObject({
      key: 'restored',
      params: { title: 'Back again' },
    });
    expect(
      describeEntry(
        E({
          kind: 'purge',
          ids: ['c1', 'c2'],
          details: { titles: ['A', 'B'] },
        }),
      ),
    ).toMatchObject({ key: 'purgedMany', params: { count: 2 } });
  });

  it('describes archive of cards and lanes', () => {
    expect(
      describeEntry(
        E({
          kind: 'setArchived',
          details: {
            after: { items: [{ ...item('c1', 'Done thing'), archived: true }] },
          },
        }),
      ).key,
    ).toBe('archived');
    expect(
      describeEntry(
        E({
          kind: 'setArchived',
          details: {
            after: { items: [{ ...item('c1', 'X'), archived: false }] },
          },
        }),
      ).key,
    ).toBe('unarchived');
    expect(
      describeEntry(
        E({
          kind: 'setArchived',
          details: {
            after: { items: [], lanes: [{ lane: 'Old', archived: true }] },
          },
        }),
      ),
    ).toMatchObject({ key: 'laneArchived', params: { lane: 'Old' } });
  });

  it('describes lanes and board changes', () => {
    expect(
      describeEntry(
        E({
          kind: 'updateLane',
          details: { before: { lane: 'Todo', patch: { name: 'To do' } } },
        }),
      ),
    ).toMatchObject({
      key: 'laneRenamed',
      params: { from: 'Todo', to: 'To do' },
    });
    expect(describeEntry(E({ kind: 'createLane', details: { after: { lane: 'QA' } } }))).toMatchObject({ key: 'laneCreated', params: { lane: 'QA' } });
    expect(
      describeEntry(
        E({
          kind: 'updateBoard',
          details: { after: { patch: { name: 'Roadmap' } } },
        }),
      ),
    ).toMatchObject({ key: 'boardRenamed' });
  });

  it('describes cross-board moves using the board resolver', () => {
    const e = E({
      kind: 'moveBoard',
      board: 'b2',
      details: { titles: ['X'], from: 'b1', to: 'b2' },
    });
    expect(describeEntry(e, (id) => (id === 'b1' ? 'Work' : null))).toMatchObject({ key: 'movedIn', params: { board: 'Work', title: 'X' } });
    expect(
      describeEntry(
        E({
          kind: 'moveBoard',
          board: 'b1',
          details: { from: 'b1', to: 'bzz' },
        }),
      ).key,
    ).toBe('movedOutUnknown');
  });

  it('describes external and remote changes', () => {
    expect(
      describeEntry(
        E({
          kind: 'externalEdit',
          origin: 'external',
          details: { titles: ['A'], removed: 0 },
        }),
      ).key,
    ).toBe('external');
    expect(
      describeEntry(
        E({
          kind: 'externalEdit',
          origin: 'external',
          details: { titles: [], removed: 2 },
        }),
      ).key,
    ).toBe('externalRemoved');
    const r = describeEntry(
      E({
        kind: 'remoteSync',
        origin: 'remote',
        details: { service: 'jira', titles: ['PAY-12 Refunds'] },
      }),
    );
    expect(r).toMatchObject({
      key: 'remote',
      group: 'integrations',
      params: { service: 'jira' },
    });
  });

  it('describes undo and unknown kinds', () => {
    expect(describeEntry(E({ kind: 'undo', label: 'Undo: Delete card' }))).toMatchObject({ key: 'undo', params: { label: 'Delete card' } });
    expect(describeEntry(E({ kind: 'mystery', label: 'Something' }))).toMatchObject({ key: 'other', params: { label: 'Something' } });
  });
});

describe('classification helpers', () => {
  it('maps kinds and sources', () => {
    expect(kindGroup({ kind: 'edit', origin: 'you' })).toBe('edited');
    expect(kindGroup({ kind: 'jiraPush', origin: 'you' })).toBe('integrations');
    expect(sourceOf({ kind: 'move', origin: 'you' })).toBe('you');
    expect(sourceOf({ kind: 'externalEdit', origin: 'external' })).toBe('external');
    expect(
      sourceOf({
        kind: 'remoteSync',
        origin: 'remote',
        details: { service: 'Trello' },
      }),
    ).toBe('trello');
    expect(sourceOf({ kind: 'jiraSync', origin: 'remote' })).toBe('jira');
    // Service names are sanitised (they end up in CSS classes / i18n keys).
    expect(
      sourceOf({
        kind: 'remoteSync',
        origin: 'remote',
        details: { service: '<script>' },
      }),
    ).toBe('remote');
  });

  it('never exposes ids as titles', () => {
    expect(entryTitles(E({ ids: ['c1'] }))).toEqual([]);
    expect(entryCardId(E({ ids: ['kabcdef', 'cabcdef'] }))).toBe('cabcdef');
    expect(entryCardId(E({ kind: 'createLane', ids: ['cabcdef'] }))).toBeNull();
  });
});

describe('groupByDay', () => {
  it('groups newest first with today / yesterday', () => {
    const now = new Date(2026, 8, 30, 12);
    const at = (d: number, h: number) => new Date(2026, 8, d, h).toISOString();
    const g = groupByDay([{ ts: at(28, 9) }, { ts: at(30, 8) }, { ts: at(29, 20) }, { ts: at(30, 11) }, { ts: 'bad' }], now);
    expect(g.map((x) => [x.rel, x.entries.length])).toEqual([
      ['today', 2],
      ['yesterday', 1],
      ['date', 1],
    ]);
    expect(g[0].entries[0].ts).toBe(at(30, 11));
  });
});

describe('filtering', () => {
  const list = [
    E({
      kind: 'move',
      details: { before: { items: [item('c1', 'Café menu', 'Doing')] } },
    }),
    E({
      kind: 'externalEdit',
      origin: 'external',
      details: { titles: ['Budget'] },
    }),
    E({
      kind: 'remoteSync',
      origin: 'remote',
      details: { service: 'jira', titles: ['PAY-1'] },
    }),
  ];
  it('filters by group, source and accent-insensitive text', () => {
    expect(filterEntries(list, { groups: ['moved'] })).toHaveLength(1);
    expect(filterEntries(list, { sources: ['jira'] })).toHaveLength(1);
    expect(filterEntries(list, { sources: ['you', 'external'] })).toHaveLength(2);
    expect(filterEntries(list, { text: 'cafe' })).toHaveLength(1);
    expect(filterEntries(list, { text: 'doing cafe' })).toHaveLength(1);
    expect(fold('Ñandú')).toBe('nandu');
  });

  it('computes range lower bounds', () => {
    const now = new Date(2026, 8, 30, 15);
    expect(rangeFrom('all', now)).toBeUndefined();
    expect(new Date(rangeFrom('today', now)!).getDate()).toBe(30);
    expect(new Date(rangeFrom('week', now)!).getDate()).toBe(24);
  });
});

describe('daysLeft', () => {
  const now = Date.parse('2026-09-30T12:00:00Z');
  it('counts whole days remaining', () => {
    expect(daysLeft('2026-09-30T12:00:00Z', 7, now)).toBe(7);
    expect(daysLeft('2026-09-29T12:00:00Z', 7, now)).toBe(6);
    expect(daysLeft('2026-09-29T18:00:00Z', 7, now)).toBe(7);
    expect(daysLeft('2026-09-01T00:00:00Z', 7, now)).toBe(0);
    expect(daysLeft('nonsense', 7, now)).toBe(0);
    expect(daysLeft('2026-09-30T12:00:00Z', 0, now)).toBe(1);
  });
});
