'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { rustTarget, binaryName, archiveName } = require('./platform');

test('maps macOS ARM to the apple darwin target', () => {
  assert.equal(rustTarget('darwin', 'arm64'), 'aarch64-apple-darwin');
  assert.equal(binaryName('darwin'), 'karmx');
  assert.equal(
    archiveName('aarch64-apple-darwin', 'darwin'),
    'karmx-aarch64-apple-darwin.tar.gz',
  );
});

test('maps Windows x64 to the msvc zip', () => {
  assert.equal(rustTarget('win32', 'x64'), 'x86_64-pc-windows-msvc');
  assert.equal(binaryName('win32'), 'karmx.exe');
  assert.equal(
    archiveName('x86_64-pc-windows-msvc', 'win32'),
    'karmx-x86_64-pc-windows-msvc.zip',
  );
});

test('rejects unknown platforms', () => {
  assert.throws(() => rustTarget('sunos', 'x64'), /no install path/);
});
