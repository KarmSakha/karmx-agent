# Terminal, desktop, and automation upgrade

Research date: September 21, 2026.

## First delivery: expose startup context profiles

Use `karmx context` to inspect the effective profile and extension allowlist. Use
`karmx context compact` to save the existing lower-context profile,
`karmx context default` for full context, or `karmx context auto` to remove the
explicit choice and inherit the provider default. The desktop exposes the same
configuration in Settings → Extensions → Startup context. Restart the app or CLI
session after changing it so prompts and loaded extensions agree.

This exposes existing backend behavior; it does not change either agent loop.
Environment variables override saved settings. A custom
`KARMX_EXTENSIONS_ALLOWLIST` overrides the profile's default selection. Settings
are shared only when the CLI and desktop use the same configuration directory.

Compact currently allows developer, context_engine, todo, browser_preview,
computer_control, screen_task, analyze, and summon. These must also be enabled in
configuration. Other extensions, including skills and extension discovery, are
excluded unless the allowlist is customized. This is a fixed subset, not automatic
on-demand tool loading. No token or latency improvement was measured in this change.

## Comparison and next deliveries

### Terminal follow-up delivery

- `/resume` opens a searchable picker ordered by latest activity, showing session
  IDs, timestamps, message counts, and directories. `/resume <id>` opens a specific
  saved session through the existing loader. The saved mode is restored and the
  history is displayed; changing directories uses the existing resume prompt.
- `/tools` opens a searchable list of loaded tools and displays the selected
  tool's description and input schema without executing it.
- `/context` reports the active model/session, loaded tool count, context usage,
  provider-reported input/output/cache usage, and accumulated session tokens.
- The idle display identifies the model/session. During tool work, the activity
  indicator shows tool names, elapsed time, and completed calls. Each response
  includes timing and the latest model call's output tokens. Generation tok/s is
  available only when the provider supplies generation timing.
- During a run, the status display also shows provider/model, loaded tool count,
  context tokens/limit/percentage, and last reported input/output/cache counters.
  It returns after streamed output is rendered and refreshes every half second;
  usage updates on provider usage events. Missing counts show “—”. No slash command
  is needed. Tool counts use the cached catalog and refresh on usage events.

These are CLI presentation changes shared by both agent-loop paths. They have
been formatted and inspected, but require a Rust build and interactive terminal
verification before release. The existing session builder still treats some
provider/setup failures as fatal, as it does for command-line resume.

[Pi's current documentation](https://raw.githubusercontent.com/badlogic/pi-mono/main/packages/coding-agent/README.md)
describes four default tools, steering and follow-up queues, session tree navigation,
and an extensible terminal interface. These are useful reference points; the goal
is to improve KarmX's existing Rust CLI, not revive the deprecated `ui/text` project.

| Area | Observed KarmX implementation | Next delivery |
| --- | --- | --- |
| Terminal | Slash menu, model picker, context usage, prompt enhancement | Show active tool count and measured initial context; make steering versus follow-up explicit |
| Desktop | Editable/reorderable message queue and context indicator | Keyboard-accessible queue controls and a context breakdown using backend measurements |
| Startup context | Compact prompt, extension allowlist, skill listing cap | Shared on-demand discovery with a small initial catalog, retaining explicit disable/allowlist rules |
| Browser | CDP computer control, DOM elements, browser task, preview | Connection/readiness diagnostics and visible action progress tied to a specific browser target |
| Native computer | OCR + accessibility observations, Jev decisions, native actions | Permission/key readiness, visible app scope, cancel/progress events, and observed completion verification |

Before adding deferred tools, design one shared discovery contract for the legacy
and state-machine loops. Filtering startup tools alone can make capabilities
unreachable, as the existing compact allowlist demonstrates. Verify enablement,
explicit denial, discovery, and tool refresh under both loop settings.

For the interface, prefer a dense workspace: conversation, composer, a quiet status
line, and expandable tool details. Put browser/computer progress in an inspectable
panel showing the target, current action, and stop control. Keep startup context
distinct from total conversation usage and accumulated billing tokens.

## Jev integration boundary

[TypeSafe's announcement](https://typesafe.ai/blog/introducing-system-one-models-and-jev)
describes structured decisions rather than text generation. Use Jev to select
from observed controls/actions, with the main model responsible for planning and
writing. Typed output still needs outcome verification. The
[reference computer-use implementation](https://github.com/awlevin/typesafe-computer-use)
uses OCR observations and a native driver on macOS.

KarmX already has `typesafe.rs`, DOM selection in `computer_control/elements.rs`,
and the native `screen_task` runner. Extend these paths. Audit cancellation,
stale-observation handling, target identity, confidence handling, timeouts, and
post-action checks before claiming reliable browser/native automation. The first
delivery preserves these implementations; it does not validate them live.

## Verification plan

The full, compact, and subagent prompts now require real tool calls, evidence for
completion claims, readback of relevant outcomes, and stopping a step after two
failures without new evidence. Changing tools or delegating does not reset that
prompt-level limit. Dependent checks follow successful edits; asynchronous jobs
are awaited rather than duplicated. Browser guidance is conditional on available
tools. Existing prompt snapshots were updated to match the text changes. Snapshot
execution is blocked by the local macOS dependency linker error recorded below.

The runtime already installs a repetition inspector with a default threshold of
three identical consecutive calls. That is distinct from the prompt's broader
no-progress rule; these prompt edits do not add a runtime detector for alternating
failed approaches and cannot guarantee elimination of hallucinations. Custom user
prompt overrides can replace the built-in prompts. Prompt guidance reduces risk; it is not a runtime guarantee. Broader behavioral
evaluations remain outstanding.

- Check CLI parsing for inspect, compact, default, auto, and invalid values.
- With an isolated configuration directory, confirm compact/default persist and
  auto removes only the profile override. Check environment precedence.
- In desktop settings, save each option, restart, and confirm the same backend
  profile. Check save failures and keyboard operation; hide this global setting
  in recipe/session extension selectors.
- Compare the initial serialized prompt/tool tokens and time to first response
  for identical requests/providers/workspaces in full and compact modes. Report
  warm/cold runs separately and include tool discovery costs.
- Exercise coding, DOM browser work, and a bounded native task in both modes and
  both agent loops. Do not measure speed at the expense of task success.

## Build and validation (2026-09-21)

The user authorized build, verification, and push.

- Built `target/debug/karmx` with `cargo build -j 3 -p goose-cli --bin karmx`.
- Desktop typecheck and targeted ESLint passed. Vite renderer production build
  passed (output `/tmp/karmx-ux-renderer-build`); no packaged Electron app or
  release CLI binary was built or installed.
- CLI session tests: 222 passed, one ignored, with isolated configuration.
  The initial run exposed a local thinking-effort configuration dependency in an
  existing model-switch test; it passes with isolated configuration.
- Desktop tests: 56 passed across extension utilities, extension items, and the
  extension modal. Modal tests used a 30-second timeout after the default
  5-second timeout failed under build load.
- PTY smoke testing against a local mock OpenAI server verified persisted profile
  changes, live model/tool/context/token display, complete streamed response
  lines, invalid resume handling, and the tool picker under both agent loops.
  The state-machine smoke also verified switching to a new session and resuming
  the original session with its message history. Mock usage includes cache
  reads; unknown fields stay unknown. This does not validate a real provider's
  usage accuracy or browser/native automation.
- Added regression coverage for missing usage versus real zero counts, context
  overflow, and partial response lines during live status redraws.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Prompt snapshot tests could not execute: macOS rejects the compiled
  `sqlx_macros` dynamic library with “mis-aligned LINKEDIT string pool”. A clean
  rebuild of that dependency reproduced the error.
- Ran the focused reliability self-test recipe with a 12-turn limit. The configured
  provider returned HTTP 502 from `ai.karmx.dev` before executing checks, so the
  recipe did not pass (despite the existing CLI returning exit code zero).
- Full workspace tests and clippy were not run. No merge or release was performed.

