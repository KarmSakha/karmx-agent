#!/usr/bin/env node
'use strict';

const { execFileSync, spawn } = require('node:child_process');
const { existsSync } = require('node:fs');
const { join } = require('node:path');
const { binaryName } = require('../scripts/platform');

const forwarded =
  process.platform === 'win32'
    ? ['SIGINT', 'SIGTERM']
    : ['SIGHUP', 'SIGINT', 'SIGTERM'];

function vendorPath() {
  return join(__dirname, '..', 'vendor', binaryName());
}

function resolveBinary() {
  const override = process.env.KARMX_BINARY?.trim();
  if (override) {
    return override;
  }
  const vendor = vendorPath();
  if (!existsSync(vendor)) {
    execFileSync(process.execPath, [join(__dirname, '..', 'scripts', 'install.js')], {
      stdio: 'inherit',
      env: process.env,
    });
  }
  if (!existsSync(vendor)) {
    console.error(
      'karmx: native CLI is missing. Reinstall with `npm install -g karmx` (needs a GitHub release or a Rust toolchain).',
    );
    process.exit(1);
  }
  return vendor;
}

const binaryPath = resolveBinary();
const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
const handlers = new Map();

for (const signal of forwarded) {
  const handler = () => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill(signal);
    }
  };
  handlers.set(signal, handler);
  process.on(signal, handler);
}

function cleanup() {
  for (const [signal, handler] of handlers) {
    process.off(signal, handler);
  }
}

child.on('error', (error) => {
  cleanup();
  console.error(`karmx: ${error.message}`);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  cleanup();
  if (signal) {
    try {
      process.kill(process.pid, signal);
    } catch {
      process.exit(1);
    }
    return;
  }
  process.exit(code ?? 1);
});
