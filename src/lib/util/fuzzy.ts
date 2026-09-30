// Fuzzy matching with highlight ranges (subsequence with word-start bonuses).

export interface FuzzyMatch {
  score: number;
  /** Matched character indices in the target. */
  positions: number[];
}

const isWordStart = (s: string, i: number) => i === 0 || /[\s\-_/.:>]/.test(s[i - 1]) || (s[i] !== s[i].toLowerCase() && s[i - 1] === s[i - 1].toLowerCase());

export function fuzzy(query: string, target: string): FuzzyMatch | null {
  if (!query) return { score: 0, positions: [] };
  const q = query.toLowerCase();
  const tl = target.toLowerCase();
  // Fast path: contiguous substring.
  const idx = tl.indexOf(q);
  if (idx >= 0) {
    const positions = Array.from({ length: q.length }, (_, i) => idx + i);
    return { score: 1000 - idx * 2 + (isWordStart(target, idx) ? 200 : 0) - target.length * 0.1, positions };
  }
  let score = 0;
  let ti = 0;
  let prev = -2;
  const positions: number[] = [];
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi];
    if (ch === ' ') continue;
    let found = -1;
    // Prefer word starts.
    for (let j = ti; j < tl.length; j++) {
      if (tl[j] === ch && isWordStart(target, j)) {
        found = j;
        break;
      }
    }
    if (found < 0) found = tl.indexOf(ch, ti);
    if (found < 0) return null;
    score += 10;
    if (found === prev + 1) score += 15;
    if (isWordStart(target, found)) score += 25;
    score -= Math.min(found - ti, 10);
    positions.push(found);
    prev = found;
    ti = found + 1;
  }
  return { score: score - target.length * 0.2, positions };
}

/** Split `text` into segments for rendering highlights. */
export function highlightSegments(text: string, positions: number[]): { text: string; hit: boolean }[] {
  if (!positions.length) return [{ text, hit: false }];
  const set = new Set(positions);
  const out: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const hit = set.has(i);
    const last = out[out.length - 1];
    if (last && last.hit === hit) last.text += text[i];
    else out.push({ text: text[i], hit });
  }
  return out;
}
