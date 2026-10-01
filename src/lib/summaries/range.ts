// Period presets for activity summaries (pure; local time → RFC 3339 bounds).

export type RangePreset = 'today' | 'yesterday' | 'lastWorkday' | 'thisWeek' | 'lastWeek' | 'last7' | 'last30' | 'custom';

export const RANGE_PRESETS: RangePreset[] = ['today', 'yesterday', 'lastWorkday', 'thisWeek', 'lastWeek', 'last7', 'last30', 'custom'];

export interface Range {
  from: Date;
  to: Date;
}

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate(), 0, 0, 0, 0);
const endOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate(), 23, 59, 59, 999);
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds());
/** Monday-based week start. */
const weekStart = (d: Date) => startOfDay(addDays(d, -((d.getDay() + 6) % 7)));

/** Local `YYYY-MM-DD` (for `<input type="date">`). */
export function ymd(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function parseYmd(s: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return null;
  const d = new Date(+m[1], +m[2] - 1, +m[3]);
  return Number.isNaN(d.getTime()) || d.getMonth() !== +m[2] - 1 ? null : d;
}

/**
 * Resolve a preset at `now`. `custom` uses the given `YYYY-MM-DD` days
 * (inclusive, swapped if reversed); returns null when they are invalid.
 */
export function resolveRange(preset: RangePreset, now: Date, custom?: { from: string; to: string }): Range | null {
  switch (preset) {
    case 'today':
      return { from: startOfDay(now), to: now };
    case 'yesterday': {
      const y = addDays(now, -1);
      return { from: startOfDay(y), to: endOfDay(y) };
    }
    case 'lastWorkday': {
      // Monday → Friday..Sunday; Sunday/Saturday → Friday; else yesterday.
      const dow = now.getDay();
      const back = dow === 1 ? 3 : dow === 0 ? 2 : dow === 6 ? 1 : 1;
      const from = startOfDay(addDays(now, -back));
      const to = dow === 1 ? endOfDay(addDays(now, -1)) : endOfDay(from);
      return { from, to };
    }
    case 'thisWeek':
      return { from: weekStart(now), to: now };
    case 'lastWeek': {
      const ws = weekStart(now);
      return { from: addDays(ws, -7), to: endOfDay(addDays(ws, -1)) };
    }
    case 'last7':
      return { from: startOfDay(addDays(now, -6)), to: now };
    case 'last30':
      return { from: startOfDay(addDays(now, -29)), to: now };
    case 'custom': {
      let a = custom ? parseYmd(custom.from) : null;
      let b = custom ? parseYmd(custom.to) : null;
      if (!a || !b) return null;
      if (a > b) [a, b] = [b, a];
      return { from: startOfDay(a), to: endOfDay(b) };
    }
  }
}

export const toIso = (r: Range) => ({ from: r.from.toISOString(), to: r.to.toISOString() });

/** Strip Markdown to plain text (copy as text). */
export function plainText(md: string): string {
  return md
    .split('\n')
    .map((l) =>
      l
        .replace(/^#{1,6}\s+/, '')
        .replace(/^\s*[-*]\s+/, '• ')
        .replace(/\*\*(.+?)\*\*/g, '$1')
        .replace(/(^|[^*])\*(?!\s)(.+?)\*/g, '$1$2')
        .replace(/_(.+?)_/g, '$1')
        .replace(/\[\[([^\]|]+)\|([^\]]+)\]\]/g, '$2')
        .replace(/\\([\\`*_{}[\]()#+\-.!])/g, '$1'),
    )
    .join('\n')
    .trim();
}

/** Markdown → Slack mrkdwn (headings bold, `**` → `*`). */
export function slackText(md: string): string {
  return md
    .split('\n')
    .map((l) => {
      const h = /^#{1,6}\s+(.*)$/.exec(l);
      if (h) return `*${h[1].trim()}*`;
      return l.replace(/\*\*(.+?)\*\*/g, '*$1*').replace(/^(\s*)- /, '$1• ');
    })
    .join('\n')
    .trim();
}

/** Replace `[[id]]` links with card titles. */
export function resolveLinks(md: string, titleOf: (id: string) => string): string {
  return md.replace(/\[\[([a-z0-9]{3,})\]\]/gi, (_, id: string) => titleOf(id));
}
