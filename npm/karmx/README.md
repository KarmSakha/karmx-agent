# karmx

Global npm installer for the [karmX](https://github.com/KarmSakha/karmx-agent) native CLI.

```bash
npm install -g karmx
karmx --version
karmx configure
```

On npm 10+, allow the installer once if prompted:

```bash
npm install -g karmx --allow-scripts=karmx
```

The first `karmx` run also fetches the native binary if postinstall was skipped. Prebuilt GitHub release assets are used when present; otherwise it compiles from source with `cargo`. When the package is linked from a source checkout (`npm link` in `npm/karmx`), that checkout is built instead of cloning.

```bash
# from a source checkout, reuse / build the local binary
KARMX_REPO=/path/to/karmX npm install -g karmx
```

Override the installed binary with `KARMX_BINARY=/path/to/karmx`.
