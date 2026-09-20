const { FusesPlugin } = require('@electron-forge/plugin-fuses');
const { FuseV1Options, FuseVersion } = require('@electron/fuses');
const { resolve } = require('path');

const isLinuxVulkanBuild = process.env.GOOSE_DESKTOP_LINUX_VARIANT === 'vulkan';

let cfg = {
  asar: true,
  extraResource: ['src/bin', 'src/images', 'src/app-update.yml'],
  icon: 'src/images/icon',
  appBundleId: process.env.KARMX_BUNDLE_ID || 'com.karmsakha.karmx',
  // Windows specific configuration
  win32: {
    icon: 'src/images/icon.ico',
    certificateFile: process.env.WINDOWS_CERTIFICATE_FILE,
    signingRole: process.env.WINDOW_SIGNING_ROLE,
    rfc3161TimeStampServer: 'http://timestamp.digicert.com',
    signWithParams: '/fd sha256 /tr http://timestamp.digicert.com /td sha256',
  },
  // Protocol registration
  protocols: [
    {
      name: 'GooseProtocol',
      schemes: ['goose', 'karmx'],
    },
  ],
  // macOS Info.plist extensions for drag-and-drop support
  extendInfo: {
    // Document types for drag-and-drop support onto dock icon
    CFBundleDocumentTypes: [
      {
        CFBundleTypeName: 'Folders',
        CFBundleTypeRole: 'Viewer',
        LSHandlerRank: 'Alternate',
        LSItemContentTypes: ['public.directory', 'public.folder'],
      },
    ],
    // Usage descriptions for macOS TCC (Transparency, Consent, and Control)
    NSMicrophoneUsageDescription:
      'KarmX needs access to your microphone for voice dictation.',
    NSAppleEventsUsageDescription:
      'KarmX needs access to send Apple Events to control other apps on your behalf.',
  },
};

// macOS code signing and notarization via Electron Forge
// Signing is activated when APPLE_TEAM_ID is set (CI signing builds) or a
// Developer ID identity is available in the keychain (local builds).
const notaryProfile = process.env.KARMX_NOTARY_PROFILE;
const hasAppleIdCreds = process.env.APPLE_ID && process.env.APPLE_ID_PASSWORD;
const hasAscKeyCreds =
  process.env.APPLE_API_KEY && process.env.APPLE_API_KEY_ID && process.env.APPLE_API_ISSUER;
const shouldSign = Boolean(
  process.env.APPLE_TEAM_ID ||
    process.env.OSX_SIGN_IDENTITY ||
    notaryProfile ||
    hasAppleIdCreds ||
    hasAscKeyCreds
);

if (shouldSign) {
  cfg.osxSign = {
    keychain: process.env.KEYCHAIN_PATH || undefined,
    identity: process.env.OSX_SIGN_IDENTITY || undefined,
    optionsForFile: () => ({
      hardenedRuntime: true,
      entitlements: resolve(__dirname, 'entitlements.plist'),
    }),
    // Data resources are sealed by the enclosing bundle; only code needs its own signature.
    ignore: (file) => /\.(pak|png|jpe?g|gif|webp|ico|icns|woff2?|ttf|otf|pdf|mp[34]|wasm)$/i.test(file),
  };

  if (notaryProfile) {
    // Credentials stored via `xcrun notarytool store-credentials`
    cfg.osxNotarize = { keychainProfile: notaryProfile };
  } else if (hasAppleIdCreds) {
    cfg.osxNotarize = {
      appleId: process.env.APPLE_ID,
      appleIdPassword: process.env.APPLE_ID_PASSWORD,
      teamId: process.env.APPLE_TEAM_ID,
    };
  } else if (hasAscKeyCreds) {
    cfg.osxNotarize = {
      key: process.env.APPLE_API_KEY,
      keyId: process.env.APPLE_API_KEY_ID,
      keyIssuer: process.env.APPLE_API_ISSUER,
    };
  }
}

module.exports = {
  packagerConfig: cfg,
  rebuildConfig: {},
  publishers: [
    {
      name: '@electron-forge/publisher-github',
      config: {
        repository: {
          owner: process.env.GITHUB_OWNER || 'KarmSakha',
          name: process.env.GITHUB_REPO || 'karmx-agent',
        },
        prerelease: false,
        draft: true,
      },
    },
  ],
  makers: [
    {
      name: '@electron-forge/maker-zip',
      platforms: ['darwin', 'win32', 'linux'],
      config: {
        arch: process.env.ELECTRON_ARCH === 'x64' ? ['x64'] : ['arm64'],
        options: {
          icon: 'src/images/icon.ico',
        },
      },
    },
    {
      name: '@electron-forge/maker-dmg',
      platforms: ['darwin'],
      config: {
        format: 'ULFO',
        window: {
          size: { width: 640, height: 432 },
        },
        // Cline-style installer layout: app on the left, Applications on the right
        contents: (opts: { appPath: string }) => [
          { x: 140, y: 200, type: 'file', path: opts.appPath },
          { x: 500, y: 200, type: 'link', path: '/Applications' },
        ],
        background: process.env.KARMX_DMG_BACKGROUND ?? 'src/images/dmg-background.png',
      },
    },
    {
      name: '@electron-forge/maker-deb',
      config: {
        name: 'Goose',
        bin: 'Goose',
        maintainer: 'AAIF (Agentic AI Foundation)',
        homepage: 'https://goose-docs.ai/',
        categories: ['Development'],
        desktopTemplate: './forge.deb.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
          ...(isLinuxVulkanBuild ? { depends: ['libvulkan1'] } : {}),
        },
      },
    },
    {
      name: '@electron-forge/maker-rpm',
      config: {
        name: 'Goose',
        bin: 'Goose',
        maintainer: 'AAIF (Agentic AI Foundation)',
        homepage: 'https://goose-docs.ai/',
        categories: ['Development'],
        desktopTemplate: './forge.rpm.desktop',
        options: {
          icon: 'src/images/icon.png',
          prefix: '/opt',
          ...(isLinuxVulkanBuild ? { requires: ['vulkan-loader'] } : {}),
        },
      },
    },
    {
      name: '@electron-forge/maker-flatpak',
      config: {
        options: {
          id: 'io.github.block.Goose', // NOTE: kept for backwards compat with existing installs
          categories: ['Development'],
          mimeType: ['x-scheme-handler/goose'],
          icon: {
            scalable: 'src/images/icon.svg',
            '512x512': 'src/images/icon-512.png',
          },
          homepage: 'https://goose-docs.ai/',
          runtimeVersion: '25.08',
          baseVersion: '25.08',
          bin: 'Goose',
          modules: [
            {
              name: 'libbz2-shim',
              buildsystem: 'simple',
              'build-commands': [
                // Create the lib directory in the app bundle
                'mkdir -p /app/lib',
                // Point to the actual library in the 25.08 runtime
                // We use a wildcard to handle multi-arch paths (x86_64-linux-gnu, etc)
                'ln -s $(find /usr/lib -name "libbz2.so.1" | head -n 1) /app/lib/libbz2.so.1.0',
              ],
            },
            {
              name: 'git',
              buildsystem: 'simple',
              'build-commands': [
                'mkdir -p /app/bin /app/libexec/git-core',
                'cp /usr/bin/git /app/bin/git',
                'cp /usr/libexec/git-core/git-remote-https /app/libexec/git-core/git-remote-https 2>/dev/null || true',
              ],
            },
          ],
          finishArgs: [
            '--share=ipc',
            '--socket=x11',
            '--socket=wayland',
            '--device=dri',
            '--share=network',
            '--filesystem=home',
            '--talk-name=org.freedesktop.Notifications',
            '--socket=session-bus',
            '--socket=system-bus',
            // This ensures the app looks in our shim folder first
            '--env=LD_LIBRARY_PATH=/app/lib',
            '--env=GIT_EXEC_PATH=/app/libexec/git-core',
          ],
        },
      },
    },
  ],
  plugins: [
    {
      name: '@electron-forge/plugin-vite',
      config: {
        build: [
          {
            entry: 'src/main.ts',
            config: 'vite.main.config.mts',
          },
          {
            entry: 'src/preload.ts',
            config: 'vite.preload.config.mts',
          },
        ],
        renderer: [
          {
            name: 'main_window',
            config: 'vite.renderer.config.mts',
          },
        ],
      },
    },
    // Fuses are used to enable/disable various Electron functionality
    // at package time, before code signing the application
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};
