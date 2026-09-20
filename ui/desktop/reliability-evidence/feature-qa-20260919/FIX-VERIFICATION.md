# Reliability fixes — 19 September 2026

Follow-up to REPORT.md. The signed update is installed at `/Applications/KarmX.app`. Live checks below distinguish working features from the remaining native permission blocker.

## Implemented fixes

- Browser operations preserve page state when the requested URL is already open, serialize operations, recognize associated form labels, return actual screenshot images and report unresolved targets as errors. Jev target choices require confidence and valid candidate IDs.
- Context reranking preserves each supplied chunk independently and reports whether ranking was applied or a fallback was used.
- Sidekick sessions persist per parent conversation and provider/model across extension recreation. A running asynchronous helper must be collected before another brief. Subagent stream errors no longer masquerade as successful partial output.
- Native screen tasks preserve sensing errors, preflight macOS permissions and distinguish completion from failed or exhausted runs. Low-confidence completion decisions also stop as errors.
- Enhancement uses explicit rewrite-only instructions and a structured JSON retry after malformed XML, retaining the original draft on failure.
- Jev is pinned to 1.13.0, logs confidence/model and does not substitute winning probability for confidence.

## Regression results

79 targeted tests passed; one live screen test was ignored. The tests include URL state preservation, short independent context chunks, persisted helper identity and history, and structured enhancer fallback. The final screen-confidence ordering change was rebuilt and the complete targeted test set rerun successfully. The two shared subagent handler tests also passed with `GOOSE_STATE_MACHINE=1`; that is not a full live state-machine agent test.

## Live verification

| Area | Result | Evidence |
| --- | --- | --- |
| Browser form | PASS | Session 79, after repairing the disposable fixture’s single-threaded HTTP server. Repeated form URLs retained state; POST was exactly name `ORBIT Browser`, confirmed `true`; rendered page showed saved name. |
| Navigation/images | PASS | Followed details link, read ORBIT-7319. Six actual PNG image content blocks were returned; final screenshot independently viewed and saved as fixed-details.png. The assistant’s claim that image content was dropped is contradicted by the exported tool payload. Visual understanding by the main model was not independently measured. |
| Context | PASS for retrieval/reordering | Session 80: three separate chunks remained individually rankable and shipping was moved first, ranking applied true. Model retained all three chunks (3/3), so relevance filtering is not established. Retrieved current threshold 275000 and ORBIT-7319. |
| Fusion persistence | PASS | Session 81 (Lucy) used Coder child 82 for both calls. Second brief omitted the token and child recalled it with no tool calls. File contents independently checked. Lead’s parallel pre-completion file check raced with creation, then its subsequent checks confirmed the file. |
| Enhancer | PASS sampled | Both Coder and Lucy produced editable, unsent rewrites preserving checkout behavior and exactly one regression test. Details in fixed-enhancer-observations.md. |
| Native skill routing | PASS after correction | Old global clicker skill launched the standalone script in session 83 despite the prompt’s tool restriction; stopped that run. Updated installed skill to prefer integrated screen_task, respect user tool restrictions and report permission failures. Validation passed. Fresh session 84 called screen_task directly. |
| Native actuation | BLOCKED | Session 84 tool correctly returned isError true: Screen Recording missing; no actions performed. Accessibility is already enabled. User authorized granting both, but System Settings still requires Touch ID authentication before Screen Recording can be added. Calculator completion is not claimed. |

## Installation and limits

The release CLI compiled, the staged app was signed with the existing Developer ID and passed deep strict signature verification, then replaced atomically with the previous app retained under `/tmp/karmx-app-e2e/KarmX-before-fixes.app`. `cargo fmt` and `git diff --check` passed. Build warnings remain for unused legacy chunking helpers and a future incompatibility in dependency `block`.

Jev browser decisions were logged from pinned 1.13.0 with confidence values (sample target choices 0.98–0.99). No claim that every skill or native application is reliable. Full long-running asynchronous fusion and comprehensive live state-machine parity were not retested. The two-step official skill-selection recipe is still not implemented; KarmX continues using its simpler shortlist approach.

The installed global skill change is at `/Users/yamankhetan/.agents/skills/clicker/SKILL.md`, with a before-copy at `/tmp/karmx-app-e2e/clicker-before.md`. This correction is outside the repository; a snapshot is included here. The temporary HTTP server was stopped after verification. Coder remains selected; enhancement test drafts were cleared.


## Follow-up after permission approval

Screen Recording is now enabled for KarmX and the app was restarted. Session 85 reached OCR and live Jev successfully, but the foreground app was ChatGPT instead of Calculator. The run was stopped; no Calculator success is claimed. A single-app guard has been implemented to refuse a mismatched app before capture and stop if the foreground process changes during Jev planning. The guard regression passed (6 screen tests, 1 live-only ignored). The user was asked to leave Calculator foreground for the bounded retest. The signed app update was installed and passed strict deep signature verification. Live session 86 supplied app="Calculator" and was rejected because KarmX was foreground, with isError true and no actions. This verifies the guard; successful Calculator actuation remains pending a foreground window handoff. The complete targeted suite was rerun: 80 passed, 1 live-only ignored.
