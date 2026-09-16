'use strict';

const TARGETS = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'win32-arm64': 'aarch64-pc-windows-msvc',
};

function rustTarget(platform = process.platform, arch = process.arch) {
  const key = `${platform}-${arch}`;
  const target = TARGETS[key];
  if (!target) {
    const supported = Object.keys(TARGETS).join(', ');
    throw new Error(`karmx has no install path for ${key}. Supported: ${supported}.`);
  }
  return target;
}

function binaryName(platform = process.platform) {
  return platform === 'win32' ? 'karmx.exe' : 'karmx';
}

function archiveName(target, platform = process.platform) {
  return platform === 'win32'
    ? `karmx-${target}.zip`
    : `karmx-${target}.tar.gz`;
}

module.exports = { TARGETS, rustTarget, binaryName, archiveName };
