---
name: clicker
description: Drive the whole Mac screen toward a plain-English goal — works on ANY app (Discord, Finder, Slack), not just Chrome. OCR reads the screen, Jev picks each action, the local coder model writes free text. Use for "open X and do Y" GUI tasks that computer_control cannot reach.
---

# clicker — screen-level computer use

Jev-driven computer use through KarmX’s integrated `screen_task` tool.
A standalone fallback also exists at `~/Desktop/typesafe-computer-use`.
It captures the frontmost window, OCRs it (macOS Vision), walks the
accessibility tree, asks Jev which action comes next, and acts. Unlike
`computer_control` (Chrome/CDP only), it drives **any** macOS app.

## When to use

- "open Discord and find a jobs server" — native apps ARE in scope here
- multi-step GUI tasks across apps where CDP cannot see anything
- NOT for reading a webpage's DOM (use `page_elements`/`browser_task` first —
  structured DOM beats OCR when Chrome is the target)

## Integrated KarmX path

When `screen_task` is available, call that tool directly with the user's goal
and a bounded `steps`. For a task confined to one app, supply `app` with its
exact name (for example `Calculator`); this guards against operating another
foreground app. Use `dry_run: false` for an authorized action task.
Do not run a shell script to duplicate the built-in tool. The user's restrictions
on tools take precedence over this skill.

If the tool reports a missing macOS permission, preserve that exact reason and
stop until the permission is granted. Do not bypass a permission failure with
another tool or claim that an intended calculation was actually displayed.
Confirm completion against the observed screen text and the tool's stop reason.

For browser DOM work, prefer `page_elements` and `browser_task`; use screen
control when the native surface or browser chrome is actually needed.

## Standalone fallback

Use this only when the integrated tool is unavailable and shell-based computer
use is permitted by the user.

```bash
cd ~/Desktop/typesafe-computer-use
uv run clicker "<the user's goal in plain English>" --act --steps 12
```

- `--act` is required to actually click/type; without it the run is a dry
  single-step print.
- `--steps N` caps steps (default 12), `--delay 3` slows it, `--min-confidence`
  tunes the abort threshold (default 0.4).
- Always `cd` into the project first — `.env` (keys) is loaded from the
  working directory.

## Safety

- The run moves the real mouse and types into whatever is focused. Warn the
  user before a long `--act` run.
- Abort: Ctrl-C in the terminal, or slam the mouse into the top-left corner.
- It self-stops on `done`, low confidence, or two consecutive no-ops.

## Environment

`.env` in the project dir already holds `TYPESAFE_API_KEY` (Jev decisions),
`CLICKER_WRITER_BASE_URL`/`CLICKER_WRITER_API_KEY`/`CLICKER_WRITER_MODEL=coder`
(free text via the local coder endpoint — no Anthropic key needed).
The host app or terminal needs Screen Recording and Accessibility permissions;
verify them rather than assuming another app’s permissions apply.
