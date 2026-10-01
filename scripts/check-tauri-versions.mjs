#!/usr/bin/env node
// Fails when an npm @tauri-apps/* package and its Rust crate differ in
// major.minor (the same rule `tauri build` enforces). Run in CI so the
// mismatch is caught before a release build.
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
const lock = readFileSync('Cargo.lock', 'utf8');
const crates = new Map();
for (const m of lock.matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)) crates.set(m[1], m[2]);

const mm = (v) => v.replace(/^[^\d]*/, '').split('.').slice(0, 2).join('.');
const pairs = Object.keys(pkg.dependencies ?? {})
  .filter((n) => n.startsWith('@tauri-apps/'))
  .map((n) => [n, n === '@tauri-apps/api' ? 'tauri' : `tauri-plugin-${n.slice('@tauri-apps/plugin-'.length)}`]);

let bad = 0;
for (const [npm, crate] of pairs) {
  let installed;
  try {
    installed = JSON.parse(readFileSync(`node_modules/${npm}/package.json`, 'utf8')).version;
  } catch {
    installed = pkg.dependencies[npm];
  }
  const rust = crates.get(crate);
  if (!rust) continue;
  if (mm(installed) !== mm(rust)) {
    console.error(`✗ ${npm} ${installed} ≠ ${crate} ${rust}`);
    bad++;
  } else console.log(`✓ ${npm} ${installed} ~ ${crate} ${rust}`);
}
process.exit(bad ? 1 : 0);
