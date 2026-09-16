<div align="center">

# karmX

**The coding agent that sees your app, sharpens your prompt, and keeps the right context in the room.**

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

https://github.com/user-attachments/assets/bceeddd4-3007-49da-bc09-d18d94224876

```bash
npm install -g karmx && karmx configure
```

</div>

karmX is a native coding agent for the terminal. You bring any OpenAI-compatible model. It brings the loop around that model: prompt enhancement, a context engine, a live browser, a second agent when you need one, and compaction that starts before the window fills.

## Why this agent

| You get | Instead of |
|---|---|
| A prompt enhancer on **Ctrl+P** | Sending a vague ask and hoping the model guesses |
| A context engine that finds, ranks, and budgets code | Dumping the repo into the prompt or missing the file that matters |
| A **rendered** browser preview — DOM and console | Reading source and imagining what the page does |
| Computer control in a real Chrome | “Here’s the CSS, you click it” |
| Local fusion: lead + sidekick, two models, one session | One overloaded window doing planning and grunt work |
| Predictive compaction | Hitting the token wall mid-task |
| Your endpoint, your keys, your models | A vendor-locked default |

These capabilities run **inside the agent process**. No extra servers to babysit for the core loop.

## Prompt enhancer

Type the messy version. Press **Ctrl+P**. karmX rewrites it into a specific, unambiguous instruction using recent conversation and your working directory — then puts the result back in the prompt so you can edit and send.

The agent can also call `enhance_prompt` itself when a request is too thin to act on.

It fails open: if enhancement misses, you keep the original draft. Nothing you typed is lost.

## Context engine

Three jobs, one engine, on the model you already configured:

- **Retrieve** — search the workspace inside a character budget, so the agent reads the files that match the task
- **Rerank** — keep the spans that matter, drop the noise
- **Enhance** — turn a fuzzy request into something executable

Turn on auto-retrieve with `KARMX_CONTEXT_AUTO_RETRIEVE` if you want that search on every send, not only when the model asks.

## Browser preview

Point karmX at a running dev server. It proxies the page, injects a bridge, and returns what actually rendered — the DOM and the console — not a guess from source.

Checkout flows, empty states, and “why is this button dead” stop being archaeology.

## Computer control

When looking is not enough, karmX drives Chrome over CDP: click, type, scroll, drag, screenshot. Preview observes. Control acts.

## Local fusion

Pair a lead model with a sidekick. The lead plans and talks to you. The sidekick implements and verifies on its own context window — optionally a cheaper or faster model.

You still see one agent. Configure the pair with `karmx configure` or:

```bash
export KARMX_SIDEKICK_PROVIDER=your-fast-provider
export KARMX_SIDEKICK_MODEL=your-fast-model
```

## Predictive compaction

Summaries start in the background while there is still headroom. The ready summary applies before the window is full. Long sessions stay sharp instead of stalling on a hard limit.

## Your models

No baked-in model. Point karmX at OpenAI, Azure, OpenRouter, vLLM, Ollama, LM Studio, or any `/v1/chat/completions` host.

```bash
export KARMX_BASE_URL=https://api.openai.com
export KARMX_API_KEY=sk-your-key
export KARMX_MODEL=gpt-4o
```

Or run `karmx configure` and drop a JSON provider in `~/.config/karmx/custom_providers/` — see [`karmx_custom.json.example`](karmx_custom.json.example).

## Also in the box

- MCP servers, skills, and recipes
- `@karmx` / `@kx` shell aliases via `karmx term init`
- Desktop app from this repo when you want a window instead of a TTY

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

Prebuilt binaries come from GitHub Releases when available; otherwise the installer compiles from source (needs [Rust](https://rustup.rs)). From a checkout:

```bash
KARMX_REPO="$PWD" npm install -g ./npm/karmx
```

Then, in a project:

```bash
karmx
```

Type a rough task, press **Ctrl+P** to enhance, Enter to run.

## From source

```bash
source bin/activate-hermit
cargo build --release --bin karmx
```

## License

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
