#!/usr/bin/env node
'use strict';

const { execFileSync, spawnSync } = require('node:child_process');
const {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');
const { archiveName, binaryName, rustTarget } = require('./platform');

const REPO = process.env.KARMX_GITHUB_REPO || 'KarmSakha/karmx-agent';
const GIT_URL = `https://github.com/${REPO}.git`;
const pkg = require('../package.json');
const vendorDir = join(__dirname, '..', 'vendor');
const dest = join(vendorDir, binaryName());

function log(message) {
  console.log(`[karmx] ${message}`);
}

function commandExists(name) {
  const probe = process.platform === 'win32' ? 'where' : 'which';
  const result = spawnSync(probe, [name], { stdio: 'ignore' });
  return result.status === 0;
}

async function download(url, file) {
  const response = await fetch(url, {
    redirect: 'follow',
    headers: { 'User-Agent': 'karmx-npm' },
  });
  if (!response.ok) {
    throw new Error(`${url} -> HTTP ${response.status}`);
  }
  writeFileSync(file, Buffer.from(await response.arrayBuffer()));
}

function extractArchive(archive, outDir) {
  mkdirSync(outDir, { recursive: true });
  if (archive.endsWith('.zip')) {
    if (process.platform === 'win32') {
      execFileSync(
        'powershell.exe',
        [
          '-NoProfile',
          '-Command',
          `Expand-Archive -Force -Path "${archive}" -DestinationPath "${outDir}"`,
        ],
        { stdio: 'inherit' },
      );
      return;
    }
    execFileSync('unzip', ['-o', archive, '-d', outDir], { stdio: 'inherit' });
    return;
  }
  execFileSync('tar', ['-xzf', archive, '-C', outDir], { stdio: 'inherit' });
}

function finishBinary(source) {
  mkdirSync(vendorDir, { recursive: true });
  copyFileSync(source, dest);
  if (process.platform !== 'win32') {
    chmodSync(dest, 0o755);
  }
}

async function installFromRelease() {
  const target = rustTarget();
  const name = archiveName(target);
  const urls = [
    process.env.KARMX_BINARY_URL,
    `https://github.com/${REPO}/releases/download/v${pkg.version}/${name}`,
    `https://github.com/${REPO}/releases/latest/download/${name}`,
  ].filter(Boolean);

  const work = mkdtempSync(join(tmpdir(), 'karmx-dl-'));
  try {
    let lastError;
    for (const url of urls) {
      const archive = join(work, name);
      try {
        log(`downloading ${url}`);
        await download(url, archive);
        extractArchive(archive, work);
        const extracted = join(work, binaryName());
        if (!existsSync(extracted)) {
          throw new Error(`archive did not contain ${binaryName()}`);
        }
        finishBinary(extracted);
        log(`installed ${dest}`);
        return true;
      } catch (error) {
        lastError = error;
      }
    }
    throw lastError || new Error('no release URL succeeded');
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function repoRoot() {
  return process.env.KARMX_REPO || process.env.npm_config_karmx_repo || '';
}

// `npm link` / `npm install -g ./npm/karmx` run this script from inside a checkout.
function enclosingCheckout() {
  const root = join(__dirname, '..', '..', '..');
  return existsSync(join(root, 'crates', 'goose-cli', 'Cargo.toml')) ? root : '';
}

function installFromRepo(root) {
  const built = join(root, 'target', 'release', binaryName());
  if (existsSync(built)) {
    log(`using existing binary at ${built}`);
    finishBinary(built);
    return;
  }
  if (!commandExists('cargo')) {
    throw new Error(`cargo is not on PATH (needed to build ${root})`);
  }
  log(`building karmx from ${root}`);
  const result = spawnSync(
    'cargo',
    ['build', '--release', '--bin', 'karmx'],
    { cwd: root, stdio: 'inherit' },
  );
  if (result.status !== 0) {
    throw new Error(`cargo build failed with status ${result.status}`);
  }
  if (!existsSync(built)) {
    throw new Error(`expected ${built} after cargo build`);
  }
  finishBinary(built);
}

function installFromCargoGit() {
  if (!commandExists('cargo')) {
    throw new Error('cargo is not on PATH');
  }
  const root = mkdtempSync(join(tmpdir(), 'karmx-cargo-'));
  log(`no GitHub release yet; compiling from ${GIT_URL} (this takes a while)`);
  try {
    const result = spawnSync(
      'cargo',
      [
        'install',
        '--git',
        GIT_URL,
        '--locked',
        'goose-cli',
        '--bin',
        'karmx',
        '--root',
        root,
        '--force',
      ],
      { stdio: 'inherit' },
    );
    if (result.status !== 0) {
      throw new Error(`cargo install failed with status ${result.status}`);
    }
    const installed = join(root, 'bin', binaryName());
    if (!existsSync(installed)) {
      throw new Error(`cargo install did not produce ${installed}`);
    }
    finishBinary(installed);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

async function main() {
  mkdirSync(vendorDir, { recursive: true });
  if (existsSync(dest) && !process.env.KARMX_FORCE_INSTALL) {
    log(`already installed at ${dest}`);
    return;
  }

  const root = repoRoot();
  if (root) {
    installFromRepo(root);
    return;
  }

  try {
    await installFromRelease();
    return;
  } catch (error) {
    log(`prebuilt binary unavailable (${error.message})`);
  }

  const checkout = enclosingCheckout();
  if (checkout) {
    installFromRepo(checkout);
    return;
  }

  try {
    installFromCargoGit();
  } catch (error) {
    console.error(`
karmx could not install the native CLI.

Tried:
  1. GitHub release from ${REPO}
  2. cargo install --git ${GIT_URL} --locked goose-cli --bin karmx

${error.message}

Install a Rust toolchain (https://rustup.rs) and retry, or build from a checkout:

  git clone https://github.com/${REPO}.git
  KARMX_REPO=/path/to/karmX npm install -g karmx
`);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(`[karmx] ${error.message}`);
  process.exit(1);
});
