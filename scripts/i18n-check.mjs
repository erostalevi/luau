// i18n checks. Usage: pnpm i18n [en|es|pt]
//   en      every static key used in src/ (`t('…')`, command `title: '…'`) exists.
//   es, pt  the same, resolved in that locale only (no English fallback), plus
//           parity with English: every English key exists, plural forms match,
//           and translations don't use placeholders English doesn't define
//           (they would render literally). Dropped placeholders and extra
//           keys are warnings.
// Dictionaries are `src/lib/i18n/<locale>.ts` merged with `parts/*.<locale>.ts`.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = new URL('../src/', import.meta.url).pathname;
const i18nDir = join(root, 'lib/i18n');
const partsDir = join(i18nDir, 'parts');
const LOCALES = ['en', 'es', 'pt'];

const locale = process.argv[2] ?? 'en';
if (!LOCALES.includes(locale)) {
  console.error(`unknown locale "${locale}"; valid: ${LOCALES.join(', ')}`);
  process.exit(2);
}

// ── keys used in the source ────────────────────────────────────────────────
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

// ── dictionaries ───────────────────────────────────────────────────────────
const isObj = (v) => typeof v === 'object' && v !== null;
const merge = (a, b) => {
  const out = { ...a };
  for (const [k, v] of Object.entries(b)) out[k] = isObj(v) && isObj(out[k]) ? merge(out[k], v) : v;
  return out;
};
async function load(loc) {
  let dict = (await import(pathToFileURL(join(i18nDir, `${loc}.ts`)).href)).default;
  for (const f of readdirSync(partsDir).sort()) {
    if (f.endsWith(`.${loc}.ts`)) dict = merge(dict, (await import(pathToFileURL(join(partsDir, f)).href)).default);
  }
  return dict;
}
/** Dot-path lookup that also matches object keys containing dots (setting ids). */
const get = (dict, key) => {
  const parts = key.split('.');
  let cur = dict;
  let i = 0;
  while (i < parts.length) {
    if (!isObj(cur)) return undefined;
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

const dict = await load(locale);
const errors = [];
const warnings = [];

for (const k of [...used].sort()) if (get(dict, k) === undefined) errors.push(`missing (used in src): ${k}`);

if (locale !== 'en') {
  const en = await load('en');
  const PLURAL = new Set(['zero', 'one', 'two', 'few', 'many', 'other']);
  const isPlural = (v) => isObj(v) && Object.keys(v).length > 0 && Object.keys(v).every((k) => PLURAL.has(k));
  const ph = (s) => new Set([...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
  const walk = (a, b, path) => {
    for (const [k, av] of Object.entries(a)) {
      const p = path ? `${path}.${k}` : k;
      const bv = isObj(b) ? b[k] : undefined;
      if (bv === undefined) {
        errors.push(`missing: ${p}`);
      } else if (typeof av === 'string') {
        if (typeof bv !== 'string') errors.push(`type mismatch (expected text): ${p}`);
        else {
          const ea = ph(av);
          const eb = ph(bv);
          for (const x of eb) if (!ea.has(x)) errors.push(`unknown placeholder {${x}}: ${p}`);
          for (const x of ea) if (!eb.has(x)) warnings.push(`dropped placeholder {${x}}: ${p}`);
        }
      } else if (!isObj(bv)) {
        errors.push(`type mismatch (expected ${isPlural(av) ? 'plural forms' : 'object'}): ${p}`);
      } else {
        // Plurals: every English form (incl. `other`) is required; a locale may
        // add CLDR forms such as `many` without a warning.
        walk(av, bv, p);
      }
    }
    if (isObj(a) && isObj(b) && !isPlural(a)) {
      for (const k of Object.keys(b)) if (!(k in a)) warnings.push(`extra key (not in en): ${path ? `${path}.${k}` : k}`);
    }
  };
  walk(en, dict, '');
}

for (const w of warnings) console.warn(`warn  ${w}`);
if (errors.length) {
  console.log(`${errors.length} i18n problem(s) in ${locale}:\n` + errors.join('\n'));
  process.exit(1);
}
console.log(
  locale === 'en'
    ? `i18n en: all ${used.size} static keys present`
    : `i18n ${locale}: all ${used.size} static keys present, full parity with en${warnings.length ? ` (${warnings.length} warning(s))` : ''}`,
);
