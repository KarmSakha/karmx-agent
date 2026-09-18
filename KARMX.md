# karmX

A fork of [goose](https://github.com/block/goose) (Apache-2.0) with a native
browser preview and a native context engine, plus custom-model support.

Branch `karmx` off upstream `main`. `upstream` is `block/goose`, so future goose
releases stay pullable:

```bash
git fetch upstream && git rebase upstream/main
```

## What this fork adds

Everything runs **in the agent process**. These are platform extensions, not MCP
servers — no subprocess, no stdio boundary, no protocol hop.

| module | tools | what it does |
|---|---|---|
| `crates/goose/src/agents/platform_extensions/browser_preview/` | `browser_preview`, `close_browser_preview` | proxies a dev server, injects a page bridge, returns the **rendered** DOM and console output |
| `crates/goose/src/agents/platform_extensions/context_engine/` | `codebase_retrieval`, `enhance_prompt`, `rerank_context` | budgeted workspace search, prompt enhancement, relevance ranking |
| `crates/goose/src/agents/platform_extensions/computer_control/` | `computer_control` | drives a real Chrome over CDP (click, type, scroll, drag, screenshot); needs Chrome started with `--remote-debugging-port=<port>` (default 9222) |
| `crates/goose/src/agents/local_fusion/` + `sidekick` tool in `platform_extensions/summon.rs` | `sidekick` | persistent lead/sidekick split ("Local Fusion"): a second concurrent agent with its own model and context window; concealed from the user |

Both call models through `PlatformExtensionContext`, so they use **whatever
provider and model the session selected** — including a custom OpenAI-compatible
endpoint.

### Other changes

- **Binary renamed** `goose` → `karmx` (`crates/goose-cli/Cargo.toml`)
- **Config root separated** — `karmX`/`karmx` instead of `Block`/`goose`, so this
  fork never reads or writes an existing goose install
  (`crates/goose/src/config/paths.rs`). `KARMX_PATH_ROOT` takes precedence;
  `GOOSE_PATH_ROOT` still works.
- **Keychain separated** — secrets are stored under keyring service `karmx`
  (not `goose`), so `karmx configure` never touches a goose install's keychain
  entry.
- **System prompt** — the upstream identity line is replaced and behavioural
  guidance was merged in from a separate analysis
  (`crates/goose/src/prompts/system.md`)
- **`karmx update`** rebuilds from source (`cargo build --release --bin karmx`)
  instead of downloading upstream release archives; `--canary` builds a dirty
  working tree, `--reconfigure` runs `karmx configure` afterwards.
- **`karmx term init <shell>`** generates shell integration with `@karmx` /
  `@kx` aliases (replaces `@goose` / `@g`).

## Environment variables

Every upstream `GOOSE_*` env var has a `KARMX_*` alias that takes precedence;
the `GOOSE_*` name still works as a fallback so existing setups keep working:

| setting | preferred | fallback |
|---|---|---|
| provider | `KARMX_PROVIDER` | `GOOSE_PROVIDER` |
| model | `KARMX_MODEL` | `GOOSE_MODEL` |
| agent mode | `KARMX_MODE` | `GOOSE_MODE` |
| config root | `KARMX_PATH_ROOT` | `GOOSE_PATH_ROOT` |
| keyring off | `KARMX_DISABLE_KEYRING` | `GOOSE_DISABLE_KEYRING` |
| telemetry off | `KARMX_TELEMETRY_OFF` | `GOOSE_TELEMETRY_OFF` |
| serve secret | `KARMX_SERVER__SECRET_KEY` | `GOOSE_SERVER__SECRET_KEY` |
| subagent provider/model | `KARMX_SUBAGENT_PROVIDER` / `KARMX_SUBAGENT_MODEL` | `GOOSE_SUBAGENT_*` |
| recipes dir | `KARMX_RECIPE_PATH` | `GOOSE_RECIPE_PATH` |
| roam relays | `KARMX_ROAM_RELAYS` | `GOOSE_ROAM_RELAYS` |

The rule is mechanical: any `GOOSE_<name>` env var is read as
`KARMX_<name>` first, then `GOOSE_<name>`. Keys written inside
`config.yaml` keep their `GOOSE_*` names for compatibility — only the
environment-variable spellings gained the `KARMX_` prefix.

## Custom models

karmX does not ship a baked-in local model. Point it at **your**
OpenAI-compatible endpoint — OpenAI, Azure, OpenRouter, vLLM, Ollama, LM Studio,
or anything else that speaks `/v1/chat/completions`.

```bash
export KARMX_BASE_URL=https://api.openai.com
export KARMX_API_KEY=sk-your-key
export KARMX_MODEL=gpt-4o
```

Or any other host:

```bash
export KARMX_BASE_URL=https://your-endpoint.example
export KARMX_API_KEY=your-key
export KARMX_MODEL=your-model
```

You can also add a declarative provider with `karmx configure`, or drop a JSON
file in `~/.config/karmx/custom_providers/` — see `karmx_custom.json.example`.
`dynamic_models: true` means any model name is accepted.

### Prompt profile

`KARMX_PROMPT_PROFILE` (env var or `config.yaml` key) selects what the model
receives before the first user word. `compact` renders the short
`system.compact.md` prompt and restricts sessions to a small built-in
extension allowlist — meant for small or heavily quantized local models.
Unset uses the default profile. Precedence: explicit `KARMX_PROMPT_PROFILE` →
the active provider's `prompt_profile` JSON field → default, so a custom
provider can declare `"prompt_profile": "compact"` once instead of setting
the env var. `KARMX_EXTENSIONS_ALLOWLIST` overrides the built-in list, and
`KARMX_SKILLS_PROMPT_LIMIT` caps the skills listing when `skills` is allowed.

Under compact, `summon` offers only the `sidekick` tool (when a sidekick
provider or model is configured), `context_engine` offers only
`codebase_retrieval`, and `skills` is off the list.

### Sidekick model

The sidekick (Local Fusion) can run on a different provider and model from the
lead. Each setting works as an env var or as a key in
`~/.config/karmx/config.yaml` (env wins):

- `KARMX_SIDEKICK_PROVIDER` — provider name for the sidekick, e.g. a custom
  provider from `custom_providers/`; unset = the lead's provider.
- `KARMX_SIDEKICK_MODEL` — model uid for the sidekick; unset = the lead's model.

The lead's system prompt carries the sidekick guidance only in requests that
offer the `sidekick` tool (main sessions with the summon extension, outside
chat mode).

```yaml
# ~/.config/karmx/config.yaml
GOOSE_PROVIDER: openai
GOOSE_MODEL: gpt-4o
KARMX_SIDEKICK_PROVIDER: karmx_custom
KARMX_SIDEKICK_MODEL: your-fast-model
```

`KARMX_SUBAGENT_PROVIDER` / `KARMX_SUBAGENT_MODEL` (or their `GOOSE_*`
fallbacks), if set, take priority over these — that is the existing delegate
resolution order, unchanged.

## Install

```bash
npm install -g karmx
karmx configure
```

On npm 10+, allow the installer if prompted: `npm install -g karmx --allow-scripts=karmx`.

From this checkout, reuse the local binary:

```bash
KARMX_REPO="$PWD" npm install -g ./npm/karmx
```

## Build

```bash
cargo build --release -p goose-cli   # produces target/release/karmx
```

`vendor/v8` is a workspace member but is not in the CLI dependency graph, so it
is not built.

## Tests

```bash
cargo test -p goose --lib browser_preview
cargo test -p goose --lib context_engine
cargo test -p goose --lib computer_control
cargo test -p goose --lib local_fusion
```
