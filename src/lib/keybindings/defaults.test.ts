import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { BASE, PRESETS } from './defaults';

/** Command ids declared anywhere in the UI sources (`id: 'area.name'`). */
function declaredIds(): Set<string> {
  const ids = new Set<string>();
  const walk = (d: string) => {
    for (const f of readdirSync(d)) {
      const p = join(d, f);
      if (statSync(p).isDirectory()) walk(p);
      else if (/\.(ts|svelte)$/.test(f) && !f.endsWith('.test.ts'))
        for (const m of readFileSync(p, 'utf8').matchAll(/\bid:\s*['"]([a-zA-Z0-9]+\.[a-zA-Z0-9.]+)['"]/g)) ids.add(m[1]);
    }
  };
  walk('src/lib');
  return ids;
}

describe('default keybindings', () => {
  it('only point at commands that exist', () => {
    const ids = declaredIds();
    const all = [...BASE, ...Object.values(PRESETS).flat()];
    const missing = [...new Set(all.map((b) => b.command.replace(/^-/, '')).filter((c) => !ids.has(c)))];
    expect(missing).toEqual([]);
  });
});
