#!/usr/bin/env node
// Build Luau.
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

function run(cmd, cmdArgs, env = {}) {
  const pre = Object.entries(env)
    .map(([k, v]) => `${k}=${v} `)
    .join('');
  console.log(`\n$ ${pre}${cmd} ${cmdArgs.join(' ')}`);
  if (dry) return 0;
  const r = spawnSync(cmd, cmdArgs, { stdio: 'inherit', shell: process.platform === 'win32', env: { ...process.env, ...env } });
  return r.status ?? 1;
}

function build(id) {
  const t = TARGETS[id];
  if (!t) throw new Error(`unknown target ${id}; valid: ${Object.keys(TARGETS).join(', ')}`);
  if (run('rustup', ['target', 'add', t.triple]) !== 0) return false;
  const tauriArgs = ['tauri', 'build', '--target', t.triple, '--bundles', t.bundles];
  if (debug) tauriArgs.push('--debug');
  // Updater artifacts must be signed; local builds without the key skip them.
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
    console.log('\n(no TAURI_SIGNING_PRIVATE_KEY: building without updater artifacts)');
    tauriArgs.push('-c', JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
  }
  if (run('pnpm', tauriArgs) === 0) return true;
  // The DMG step styles its window through Finder (AppleScript). Without
  // automation permission (sandboxes, SSH, CI) it fails; CI=true skips it.
  if (t.os === 'darwin' && !process.env.CI) {
    console.log('\nDMG styling failed (Finder automation not allowed?) — retrying with a plain DMG');
    return run('pnpm', tauriArgs, { CI: 'true' }) === 0;
  }
  return false;
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
