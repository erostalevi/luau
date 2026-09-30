// Lists i18n keys used in src/ (static `t('…')` / title: '…' command keys)
// and reports those missing from a dictionary. Usage: node scripts/i18n-check.mjs [en|es|pt]
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = new URL('../src/', import.meta.url).pathname;
const files = [];
(function walk(d) {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (/\.(ts|svelte)$/.test(f) && !p.includes('/i18n/')) files.push(p);
  }
})(root);

const used = new Set();
const re = /\bt\(\s*'([a-zA-Z0-9_.-]+)'/g;
const titleRe = /\btitle:\s*'((?:commands|cards|boards)\.[a-zA-Z0-9_.-]+)'/g;
for (const f of files) {
  const s = readFileSync(f, 'utf8');
  for (const m of s.matchAll(re)) used.add(m[1]);
  for (const m of s.matchAll(titleRe)) used.add(m[1]);
}

const locale = process.argv[2] ?? 'en';
const mod = await import(pathToFileURL(join(root, 'lib/i18n', `${locale}.ts`)).href).catch(() => null);
if (!mod) {
  console.log([...used].sort().join('\n'));
  process.exit(0);
}
const merge = (a, b) => {
  const out = { ...a };
  for (const [k, v] of Object.entries(b)) out[k] = typeof v === 'object' && typeof out[k] === 'object' ? merge(out[k], v) : v;
  return out;
};
let dict = mod.default;
const partsDir = join(root, 'lib/i18n/parts');
for (const f of readdirSync(partsDir)) {
  if (f.endsWith(`.${locale}.ts`)) dict = merge(dict, (await import(pathToFileURL(join(partsDir, f)).href)).default);
}
const get = (key) => {
  const parts = key.split('.');
  let cur = dict;
  let i = 0;
  while (i < parts.length) {
    if (!cur || typeof cur !== 'object') return undefined;
    let found = false;
    for (let j = parts.length; j > i; j--) {
      const k = parts.slice(i, j).join('.');
      if (k in cur) {
        cur = cur[k];
        i = j;
        found = true;
        break;
      }
    }
    if (!found) return undefined;
  }
  return cur;
};
const missing = [...used].filter((k) => get(k) === undefined).sort();
if (missing.length) {
  console.log(`${missing.length} missing keys in ${locale}:\n` + missing.join('\n'));
  process.exit(1);
}
console.log(`i18n ${locale}: all ${used.size} static keys present`);
