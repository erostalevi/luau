#!/usr/bin/env node
// Build Lull.
//   pnpm build --localtarget   build for this machine only (fastest)
//   pnpm build --release       build every target this host can build; the rest
//                              (Windows/Linux from macOS, etc.) are built by CI
//   pnpm build --target <id>   one target (macos-arm64 | macos-x64 | windows-x64 |
//                              windows-arm64 | linux-x64 | linux-arm64)
//   --debug                    debug build, --dry-run prints commands only

import { spawnSync } from 'node:child_process';
import { arch, platform } from 'node:os';

export const TARGETS = {
  'macos-arm64': { triple: 'aarch64-apple-darwin', os: 'darwin', bundles: 'app,dmg' },
  'macos-x64': { triple: 'x86_64-apple-darwin', os: 'darwin', bundles: 'app,dmg' },
  'windows-x64': { triple: 'x86_64-pc-windows-msvc', os: 'win32', bundles: 'nsis,msi' },
  'windows-arm64': { triple: 'aarch64-pc-windows-msvc', os: 'win32', bundles: 'nsis,msi' },
  'linux-x64': { triple: 'x86_64-unknown-linux-gnu', os: 'linux', bundles: 'appimage,deb,rpm' },
  'linux-arm64': { triple: 'aarch64-unknown-linux-gnu', os: 'linux', bundles: 'appimage,deb,rpm' },
};

const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const val = (f) => {
  const i = args.indexOf(f);
  return i >= 0 ? args[i + 1] : undefined;
};
const dry = has('--dry-run');
const debug = has('--debug');

function localTarget() {
  const a = arch() === 'arm64' ? 'arm64' : 'x64';
  const p = platform();
  const os = p === 'darwin' ? 'macos' : p === 'win32' ? 'windows' : 'linux';
  return `${os}-${a}`;
}

/** Targets this host can build without extra cross toolchains. */
function buildableHere(id) {
  const t = TARGETS[id];
  const p = platform();
  if (t.os !== p) return false;
  if (p === 'darwin') return true; // Apple Silicon and Intel both build on macOS
  if (p === 'win32') return true; // MSVC can cross-compile ARM64 when the ARM64 build tools are installed
  return id === localTarget(); // Linux arm64 from x64 needs a cross sysroot: leave to CI
}

function run(cmd, cmdArgs) {
  console.log(`\n$ ${cmd} ${cmdArgs.join(' ')}`);
  if (dry) return 0;
  const r = spawnSync(cmd, cmdArgs, { stdio: 'inherit', shell: process.platform === 'win32' });
  return r.status ?? 1;
}

function build(id) {
  const t = TARGETS[id];
  if (!t) throw new Error(`unknown target ${id}; valid: ${Object.keys(TARGETS).join(', ')}`);
  if (run('rustup', ['target', 'add', t.triple]) !== 0) return false;
  const tauriArgs = ['tauri', 'build', '--target', t.triple, '--bundles', t.bundles];
  if (debug) tauriArgs.push('--debug');
  return run('pnpm', tauriArgs) === 0;
}

let ids;
if (has('--release')) ids = Object.keys(TARGETS);
else if (val('--target')) ids = [val('--target')];
else ids = [localTarget()];

const results = [];
for (const id of ids) {
  if (has('--release') && !buildableHere(id)) {
    results.push([id, 'skipped (built by CI: .github/workflows/release.yml)']);
    continue;
  }
  results.push([id, build(id) ? 'ok' : 'FAILED']);
}

console.log('\nBuild summary');
for (const [id, status] of results) console.log(`  ${id.padEnd(14)} ${status}`);
if (has('--release')) {
  console.log('\nTo build every platform on GitHub Actions:  gh workflow run release.yml');
}
process.exit(results.some(([, s]) => s === 'FAILED') ? 1 : 0);
