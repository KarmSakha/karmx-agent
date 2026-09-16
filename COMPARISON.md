# Devin RE completeness vs karmX feature parity

Two separate questions, and they have different answers:

1. **Is Devin fully deassembled?** No. It is deassembled at the *interface* level.
2. **Does karmX match Devin feature-for-feature?** Mostly yes — but almost none of
   that parity comes from the deassembly.

---

## 1. What was actually recovered

| artifact | recovered | notes |
|---|---:|---|
| tool names | 33 | |
| tool **full schemas** | **2 of 33 (6%)** | only `restricted_exec`, `answer`. The rest are derived from Rust structs at runtime, so no JSON exists in the binary |
| tool descriptions | 275 | prose strings, extractable |
| agent prompts | 17 named + 74 raw literals | verbatim |
| slash commands | 59 | |
| CLI command surface | 59 help dumps (3 levels deep) | |
| feature flags | 461 | `cognition.ai/*` |
| endpoints | 89 URLs | |
| model IDs | 43 | |
| env vars | 25 | |
| internal crates | 17 named, 2054 source paths | structure, not logic |
| shipped docs | 191 man pages + 36 `.mdx` | **full text** — these ship with the binary |
| permission modes | 4 + `autonomous` | |
| sandbox design | seatbelt / bwrap + scopes | |
| compaction design | `spawn,apply,hard` + epochs | |
| browser-preview protocol | **complete** | JS bridge (37 KB) + protobuf field numbers |
| wiki client flow | complete | |
| wiki **generation** | **0%** | server-side; no generation prompt in the binary |

### What is *not* recovered

- **The logic of ~100 Rust crates.** This is machine code. There is no Rust
  decompiler that yields working source. Every "how it works" answer in the
  analysis is inferred from strings, struct field names, panic paths, and
  embedded prompts — not read from code.
- **Wiki generation.** Server-side. No prompt, no pipeline, nothing to extract.
- **Backend API protocol.** Endpoint *names* are known (`cognition.ai/wiki/status`
  etc.); request/response semantics are only partly inferable.
- **The semantic index.** Server-side and embedding-based; the binary contains no
  embedding or indexing code.
- **The Blitz patch.** Paths show `target/patch/blitz-dom-0.3.0-beta.1/`, so
  Cognition patched the upstream crate — *what* they changed is unknown.
- **Cloud / DRS implementation.** Only the CLI surface (`cloud drs …`) was mapped.
- **31 of 33 tool schemas.**

**Verdict: a thorough interface + prompt + protocol deassembly, not a code
deassembly.** That is the ceiling for a stripped release binary.

---

## 2. Feature comparison

karmX is a fork of goose, so most rows reflect **goose's** implementation, not
Devin's. Parity in a row means "a feature of that name exists", not "it behaves
identically".

| Devin feature | karmX | source | notes |
|---|---|---|---|
| Agent loop | Yes | goose | different loop, same shape |
| Tool set | Partial | goose | different tools; 33 Devin names not reused |
| Subagents | Yes | goose | 27 files; Devin's 6 profiles not ported |
| Sidekick / Local Fusion | Partial | goose | goose has subagents, not the persistent sidekick pattern |
| Skills | Yes | goose | 14 files |
| Rules | Yes | goose | 4 files |
| Hooks (lifecycle) | Yes | goose | 42 files |
| MCP client | Yes | goose | `rmcp`, 198 files. Devin hand-rolled this |
| Plugins | Yes | goose | |
| Sandbox | Partial | goose | present, shallow (2 files) |
| Permission modes | Yes | goose | 6 files; Devin's `smart` classifier not ported |
| Compaction | Yes | goose | 61 files + `goose-context-management` |
| Browser preview | **Yes** | **ported** | protocol + JS bridge verbatim; **no Blitz renderer** |
| Prompt enhancer | **Yes** | **ported** | verbatim prompt, fail-open |
| Context reranking | **Yes** | **ported** | verbatim prompt, recency tie-break |
| Wiki / DeepWiki | **No** | — | server-side; not portable |
| Message forest / revert | Partial | goose | revert present, tree structure not |
| Computer control | Partial | goose | Peekaboo, **macOS only** |
| Web search | Yes | goose | |
| Recipes / playbooks | Yes | goose | 84 files; ≈ Devin playbooks |
| Scheduler | Yes | goose | 64 files |
| Memory | Yes | goose | 40 files |
| ACP | Yes | goose | |
| TUI | Yes | goose | |
| Cloud / DRS | **No** | — | Cognition backend |
| **Custom model / BYO endpoint** | **Yes** | goose | 16 providers + 46 declarative. **Devin cannot do this at all** |

### The one strict win

`chat/completions` appears **0 times** in the Devin binary — it routes to
`api.devin.ai` with a server-chosen model and cannot be pointed at your own
endpoint. karmX accepts any OpenAI-compatible endpoint, including local models.

---

## 3. What in karmX is actually Devin-derived

Only two things, and neither is Devin's *code* — both are recovered *contracts*:

| module | what came from the RE |
|---|---|
| `browser_preview/` | the 37 KB page bridge (verbatim), protobuf field numbers, CSP rewriting, service path, CSRF header |
| `context_engine/` | the enhancer prompt (verbatim, from the SDK sample), the reranker prompt (verbatim), budget field names, fail-open semantics, recency tie-break |

Everything else in the fork is goose's, which is why the fork is 320 files of
mostly *vendored reference material* (`spec/`, `prompts/`) plus ~1,500 lines of
new Rust.

---

## 4. Honest summary

- **Devin is not deassembled.** Its interface, prompts, configuration surface, and
  one wire protocol were recovered. Its implementation was not, and cannot be from
  a release binary.
- **karmX covers most of Devin's feature surface**, because goose already did —
  not because the RE produced them.
- **Two features came from the RE**: native browser preview and the context
  engine (enhance + rerank + retrieval).
- **Two Devin features are unreachable**: the wiki (server-side generation) and
  cloud/DRS.
- **One feature karmX has that Devin structurally cannot**: user-selected models,
  including local and self-hosted.
