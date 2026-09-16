<div align="center">

# karmX

_your terminal coding agent — with in-process browser preview, a context engine, local fusion, and any OpenAI-compatible model_

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

https://github.com/user-attachments/assets/bceeddd4-3007-49da-bc09-d18d94224876

</div>

karmX is a fork of [goose](https://github.com/block/goose) (Apache-2.0). Same native agent — desktop, CLI, and API — plus platform features that run **in the agent process**, not as MCP servers.

**No baked-in model.** You configure the OpenAI-compatible endpoint you want.

## Install

```bash
npm install -g karmx
karmx --version
karmx configure
```

On npm 10+, allow the installer once if prompted:

```bash
npm install -g karmx --allow-scripts=karmx
```

That installs the native `karmx` CLI globally. Prebuilt binaries come from GitHub Releases when available; otherwise the installer compiles from source (needs [Rust](https://rustup.rs)). From a checkout:

```bash
KARMX_REPO="$PWD" npm install -g ./npm/karmx
```

## What this fork adds

| module | tools | what it does |
|---|---|---|
| `browser_preview` | `browser_preview`, `close_browser_preview` | proxies a dev server, injects a page bridge, returns the **rendered** DOM and console |
| `context_engine` | `codebase_retrieval`, `enhance_prompt`, `rerank_context` | budgeted workspace search, prompt enhancement, relevance ranking |
| `computer_control` | `computer_control` | drives Chrome over CDP (click, type, scroll, drag, screenshot) |
| `local_fusion` | `sidekick` | persistent lead/sidekick split: a second concurrent agent with its own model and context window |

Point both the lead and the sidekick at whatever provider and model **you** select — including a custom OpenAI-compatible host.

### Other changes

- Binary renamed `goose` → `karmx`
- Config and keychain are `karmx`, so an existing goose install is never touched
- Every `GOOSE_*` env var has a `KARMX_*` alias that takes precedence
- `@karmx` / `@kx` shell aliases (`karmx term init`)
- Predictive compaction: spawn / apply / hard token thresholds

Full notes: [KARMX.md](KARMX.md)

## Configure a model

Use OpenAI, Azure, OpenRouter, vLLM, Ollama, LM Studio, or any other `/v1/chat/completions` server.

```bash
export KARMX_BASE_URL=https://api.openai.com
export KARMX_API_KEY=sk-your-key
export KARMX_MODEL=gpt-4o
```

Or add a provider interactively:

```bash
karmx configure
```

Declarative example: [karmx_custom.json.example](karmx_custom.json.example) → `~/.config/karmx/custom_providers/`.

Give the sidekick a different model with `KARMX_SIDEKICK_PROVIDER` and `KARMX_SIDEKICK_MODEL`.

## Build

```bash
source bin/activate-hermit
cargo build --release -p goose-cli   # produces target/release/karmx
```

## Test

```bash
cargo test -p goose --lib browser_preview
cargo test -p goose --lib context_engine
cargo test -p goose --lib computer_control
cargo test -p goose --lib local_fusion
```

## License

Apache License 2.0. karmX retains goose’s `LICENSE` and copyright notices. See [NOTICE](NOTICE).
