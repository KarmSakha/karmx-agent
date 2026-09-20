# KarmX feature verification — 19 September 2026

Tested the installed `/Applications/KarmX.app` using real GUI conversations and disposable fixtures. No production implementation changes were made during this verification. This is a bounded feature test, not proof that every possible task works.

| Feature | Verdict | Observed evidence |
| --- | --- | --- |
| Browser interaction | FAIL | Form submissions contained an empty name and unchecked checkbox, twice. Required details-page verification was not completed. Session 20260919_73. |
| Native computer use | BLOCKED / FAIL | Calculator stayed at 0. `screen_task` returned `could not sense the screen` before any Jev action. Session 20260919_74. |
| Jev API | PASS, narrow probe | Three synthetic classification requests returned correct choices from `jev-1.13.0`, in 980–1038 ms, with confidence 0.78, 0.96, 1.0. These do not prove native actuation works. |
| Jev skill suggestion | PASS, sampled | Live app selected `clicker` for native Calculator interaction, injected it into turn context, and selected `none` for unrelated cases. Broader selection accuracy remains unmeasured. |
| Context retrieval | PASS, sampled | Retrieved exact policy ID, threshold and charges from fixture; follow-up retrieved changed threshold 275000 after disk edit. Session 20260919_75. |
| Context reranking | FAIL | Three short inputs were merged into one; tool returned `kept 1/1` and all irrelevant text in original order. Reproduced twice. |
| Fusion execution | PASS | Lead session 76 used karmx_lucy/lucy; helper 77 used karmx_coder/coder. Helper created exact JSON; independent parse and contents check passed. |
| Fusion persistent helper memory | FAIL | Second sidekick call created session 78, not a continuation of 77. It lacked prior brief and read the file. File recall is not persistent conversation memory. |
| Prompt enhancer | MIXED | Lucy produced a more specific unsent draft retaining purchase behavior. Coder showed a parse failure and correctly preserved original draft. |
| Unit regression checks | PASS | 44 passed, 1 ignored across context engine, Jev client, screen task, skill router and fusion. The ignored live screen test is not a pass. |

## Concrete defects and causes

1. Browser tools navigate whenever `url` is supplied, even when already on that page. The main model supplied URL on each action, resetting form state. This is an unsafe interface for ordinary multi-step form work.
2. Browser element extraction ignores associated HTML labels. The checkbox was offered to Jev as `on`, losing `QA Confirm`. With proper labels, the direct Jev probe correctly selected the checkbox.
3. Browser screenshots are reduced to a text byte count instead of being returned as image content. This prevents the main model from visually checking its actions.
4. Screen sensing suppresses the underlying error: failure can occur in foreground-app detection or capture before OCR. The assistant's explanation that OCR specifically failed was not established by the tool result. The result is incorrectly marked `isError: false`.
5. `rerank_context` combines provided chunks into 4000-character groups, then bypasses ranking when only one group remains. Short independent chunks therefore cannot be reordered or individually excluded.
6. The sidekick description promises persistence, but its handler dispatches a fresh delegate for each call. The lead also misleadingly marks the memory test successful despite acknowledging the missing context.
7. Prompt enhancement remains model-dependent: malformed output produces a parsing error. The GUI fallback preserved the user's draft; successful output remained editable and unsent.

## Jev documentation audit

Read the official [HTTP API](https://docs.typesafe.ai/api), [Choice primitive](https://docs.typesafe.ai/primitives/choice), [confidence guide](https://docs.typesafe.ai/confidence), [model reference](https://docs.typesafe.ai/models), [state guide](https://docs.typesafe.ai/concepts/state), [architecture guide](https://docs.typesafe.ai/concepts/how-to-build-with-system-one), [known limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13), and [skill-suggestion cookbook](https://docs.typesafe.ai/cookbooks/skill_suggestion). Also inspected the [original computer-use project](https://github.com/awlevin/typesafe-computer-use) cited by KarmX's implementation; this is a community implementation, not an official SDK.

- Endpoint, bearer authentication, state/questions shape and `jev-latest` are correct. Current documented alias resolves to 1.13.0; the live probe agrees.
- Jev is text-only. OCR/AX or DOM descriptions are the appropriate inputs. It is not the model that should receive browser screenshot images.
- Single-choice browser and skill calls currently discard confidence and probabilities. This is especially limiting for ambiguous action selection; suggestion-only routing does not always require a threshold. Keep confidence and probabilities available and evaluate thresholds on representative cases instead of copying universal constants.
- The official skill cookbook uses whole-roster ranking followed by detailed shortlist verification and independent fit checks. KarmX currently uses a lexical top-five shortlist and one Choice with `none`; this simpler approach is not equivalent, although the sampled skill selection worked.
- Model version and confidence should be recorded in app decision logs. Current logs contain choice/status/latency but not these fields.
- The client retries 429/5xx once with a bounded delay. Official guidance recommends backoff and honoring retry-after; short 1.5/2-second routing deadlines leave little retry room. No claim that this caused the observed form failure.
- Keep exact arithmetic, state transitions, action verification and invariants in deterministic code. Jev's documented limitations include numeric reasoning and distraction from irrelevant state.

## Scope and evidence

Evidence files in this directory include sanitized session messages (reasoning blocks omitted), actual browser POST events, direct Jev responses, app decision logs, enhancer observations and unit results. No keys are included.

Existing broader coding/task-suite results are recorded separately in `../../DESKTOP-VERIFICATION.md`; they were not rerun here. This pass did not exhaustively evaluate all skills, auto-retrieval settings, large-workspace retrieval, long-running nonblocking fusion, or every native application.

The disposable HTTP server was stopped after testing. Test drafts were cleared and Coder was restored as the selected default. Test sessions and fixtures remain as evidence.
