# KarmX desktop integration

The Electron renderer uses the Rust ACP server. Model/provider selections and agent settings are persisted by that server, rather than a separate frontend configuration.

| Capability | Desktop surface | Backend |
| --- | --- | --- |
| Models and reasoning | Composer model picker; Settings → Models | Provider inventory, defaults and session configuration over ACP |
| Context capacity and usage | Composer meter; exact capacity in Settings → Models | Config-aware context resolver before a session; runtime usage notifications during a session |
| Prompt enhancement | Enhance button; Ctrl+P in the composer | Native context-engine enhancement, including recent text and working directory; 45-second timeout retains the draft |
| Automatic code retrieval | Settings → Agent | `KARMX_CONTEXT_AUTO_RETRIEVE`; ACP prompt preprocessing and terminal retrieval |
| Local fusion | Settings → Agent | `KARMX_SIDEKICK_PROVIDER` / `KARMX_SIDEKICK_MODEL`; requires the summon extension |
| Predictive compaction | Settings → Agent; existing context controls | Rust compaction engine, with optional prepare/apply/hard token thresholds |
| Browser preview, computer control, retrieval and reranking | Extensions and agent tool output | Native extensions; the Agent page shows configured enabled state |
| Skills, recipes, scheduling and history | Sidebar | Existing ACP integrations retained |
| Tool approvals and chat modes | Existing chat settings and session controls | Existing backend permission system retained |

The new home screen offers editable starter drafts, shows the selected working directory, and uses the shared session composer. Selecting a starter never starts inference. Existing drafts and attachments are retained.

The KarmX logo is maintained as SVG in `src/images/karmx-icon.svg`, with PNG, macOS ICNS and Windows ICO exports. Light, dark and Aura themes retain their existing selection behavior.

## Initial GUI verification

- TypeScript and targeted ESLint checks pass.
- 26 targeted desktop tests pass, covering session draft preservation, provider adapters, usage notifications and agent settings.
- 7 Rust context resolver tests and 14 compaction threshold tests pass.
- ACP probe of the configured `karmx_lucy` / `lucy` model reports 262,144 tokens. The old GUI fallback was 128,000.
- Live ACP enhancement succeeds against the configured `lucy` model and returns an enhanced draft. The handler also has a 45-second fail-open timeout to retain the original draft when the model is slow or unavailable.
- Native provider refresh no longer calls the ACP-adapter-only readiness endpoint; a regression test covers this distinction.
- Default model and context metadata refresh when the desktop regains focus.

No provider credentials, security approvals, or model defaults are changed by opening the new settings page. Explicit Save is required to change the agent settings. Environment overrides retain precedence.

## Initial installed build

The signed replacement is installed at `/Applications/KarmX.app` using the existing Developer ID Application identity (team `PB5LGBYDAR`). Strict deep signature verification passes. The installed GUI was checked: it displays 262,144 tokens for `lucy` and loads the existing `Karmx Coder (GPU)` / `coder` sidekick selection. Existing sessions remain available.

Rollback copies are under `~/Library/Application Support/KarmX Backups/`: the previous application bundle and a validated ZIP archive. The app is built with the repository-supported Node 24 runtime; the system Node installation was not changed.

The installed GUI's Enhance button was exercised with a synthetic draft. It returned an enhanced prompt and the “Review it before sending” status, without creating a chat or submitting the draft. The test draft was cleared afterward.

## Reliability validation — 2026-09-19

The current signed build includes bounded model waits, bounded Chrome discovery/connection/navigation,
a separate browser tab for KarmX, and page inspection that returns visible text and exact link URLs.
Failed desktop requests now keep their error notification visible until dismissed. Both agent loops
use the same timeout implementation; empty transport heartbeats do not reset its deadline.

The final tuned `coder` configuration passed all 11 practical cases: Unicode files, Python repair,
CSV numeric output, JavaScript interval merging, multi-file refactoring, shell error recovery,
documentation, the state-machine path, a generated frontend, cited research, and SQL. Frontend
creation/completion/deletion, persistence and safe literal text were checked in a real browser.
Deadlines stayed at 180 seconds per task and 300 seconds for the frontend. Earlier failures were
retained rather than reclassified as passes.

Focused Rust checks passed: 45 reply-path tests (including four timeout regressions), 28 browser-tool
tests, 17 prompt-manager tests, six template tests, and three state-machine provider lifecycle tests.
TypeScript checking, Rust formatting and `git diff --check` passed. This is targeted validation, not
a claim that the complete repository test suite or every possible AI task passed.

The focused self-test recipe produced and ran eight interval checks, repaired its incorrect test
expectations, wrote its report, and stopped. The installed desktop browser task successfully read
Example Domain and `https://iana.org/domains/example` through `page_elements`.

Serving settings are deliberately below theoretical model capacity:

| Endpoint | Hardware/model | Serving context | Reasoning budget |
| --- | --- | --- | --- |
| `coder` (default) | RTX 5060 Ti 16 GB; Bonsai 2 27B PQ2_0 | 65,536; Q8 KV | 512 tokens |
| `lucy` | RX 9060 XT 16 GB; existing Qwen3.6 35B mixed IQ2/IQ3 | 32,768; Q4 KV | 1,024 tokens |

Both profiles explicitly bound output to 8,192 tokens. Actual GUI capacity follows those serving
settings. `coder` retrieved a planted fact from 54,074 prompt tokens in 76.94 seconds; `lucy` did so
from 28,832 in 27.52 seconds. These are retrieval probes, not proof of arbitrary long-context accuracy.
`lucy` was faster on coding/frontend tasks. It omitted a line in the original exact-text case,
then passed a clarified delimiter-based version. The same clarified version passed on `coder`
and both smaller candidate models. Both configurations subsequently passed all 11 practical task categories. The `lucy` suite
completed in 340 seconds; the corresponding `coder` run took approximately 12 minutes.
The default favors `coder`'s larger usable context, while `lucy` remains available for faster work.

Raw task outputs and failure histories are in `/tmp/karmx-reliability/`. Original server launch scripts
and local provider/config files have dated backups. The previous application bundles remain under
`~/Library/Application Support/KarmX Backups/`. No model files or credentials are committed.

### Candidate model comparison

Qwen3.5 9B Q4_K_M and Qwen3.5 4B Q6_K were downloaded at pinned Hugging Face revisions,
SHA-256 verified, and tested separately on the 8 GB RX 6600. The 9B thinking-mode coding run
hit the 120-second inactivity deadline. With thinking disabled, the 9B passed JavaScript
(101.9 s), Python repair (20.8 s), numeric CSV (10.6 s), and clarified Unicode text (8.9 s).
The 4B passed Python repair (19.8 s), numeric CSV (12.5 s), and clarified Unicode text (12.7 s).
These small samples do not establish a quality advantage over the 11-case default or prove
that adding a routing stage improves total task latency. No automatic helper routing was changed.

Serving guidance and candidate capability references:
- [PrismML's official serving guide](https://github.com/PrismML-Eng/Bonsai-demo/blob/main/README.md)
- [Qwen3.5 9B model card](https://huggingface.co/Qwen/Qwen3.5-9B)
- [Qwen3.5 4B model card](https://huggingface.co/Qwen/Qwen3.5-4B)
- [Qwen3-Coder models and deployment](https://github.com/QwenLM/Qwen3-Coder)

The measured results above came from these machines; vendor benchmarks were not treated as
local test results. Older Qwen3-Coder was considered, but no additional large download was
justified after the installed Bonsai configuration passed the complete practical suite.

### Final serving and desktop check

Bonsai's full-extension cold start initially took roughly two minutes. Increasing batch/ubatch
from 512/128 to 1024/512, limiting RAM cache to 2,048 MiB, and limiting context checkpoints to eight
reduced the measured 15,902-token desktop prompt evaluation to 16.67 seconds. Its first generation
took 2.34 seconds, and the final installed-app browser check passed with all 12 extensions enabled.
The desktop was restarted and confirmed `coder` as default with a 65,536-token capacity (displayed
as 66k). Both model endpoints are served on their existing loopback ports and existing
public routes; no additional public endpoint was created.

The 4B candidate also passed a tool-call round trip with the correct SKU and returned stock count.
Its temporary evaluation process was stopped and disabled after testing. The 9B/4B temporary app
providers and SSH test forwards were removed. Verified candidate model files and launch scripts
remain on the AMD server under `E:\LLM\reliability-20260919` for a later routing evaluation.

Machine-readable results are retained in `reliability-evidence/`. The aggregate `file_unicode`
result uses explicit BEGIN/END delimiters to avoid the original trailing-period ambiguity.
