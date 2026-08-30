# Agent instruction context budget

Status: KEL-147 implementation evidence. This report measures repository-owned agent
instructions; it does not claim that plain-text tokens equal full API billing tokens.

## Method

- Baseline: `origin/main@0a51360` before KEL-147.
- Plain text: `tiktoken 0.12.0`, `o200k_base` (GPT-5/GPT-4o mapping; this release does
  not map the GPT-5.6 model name).
- Actual local assembly: Codex CLI 0.150.1 `codex debug prompt-input` with no
  `project_doc_max_bytes` override.
- Recorded user profile, not a clean `CODEX_HOME`: `~/.codex/AGENTS.md` was empty;
  `~/.codex/config.toml` SHA-256 was
  `c5c5796cb03c920bb4b85500b743b098588caed7b884c299491795f37d6bc0cc`, with
  model `gpt-5.6-sol`, default effort `xhigh`, context window `1050000`, and four MCP
  servers. Project `.codex/config.toml` SHA-256 was
  `22bb2ec446e6c20a0b43359d1899831367f207a1c4d12aabaf1d8473f997563d`, adding Linear
  and Context7. Eval commands explicitly overrode effort to `low`.
- Official boundary: Codex concatenates root-to-working-directory AGENTS files and has
  a 32 KiB default project-doc limit. OpenAI's exact token-count endpoint additionally
  includes request roles, tools, schemas, files, and structural boundaries.
- Sources: [Codex AGENTS discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md),
  [OpenAI token counting](https://developers.openai.com/api/docs/guides/token-counting),
  and [lean prompt guidance](https://developers.openai.com/api/docs/guides/latest-model).

## Defect proved before the change

| Observation | Baseline |
|---|---:|
| Root AGENTS raw | 36,230 bytes / 8,554 tokens |
| Codex project instruction item | 32,868 bytes / 7,753 tokens |
| Root tail complete | No; cut inside current-head CodeRabbit rule |
| Nested wv AGENTS from `crates/keld-wv` | Absent |
| Local start-message plain-text tokens | about 11.8k, excluding tool schemas |

Increasing the discovery limit to 64 KiB restored nested wv instructions but raised the
automatic project item to 9,195 tokens. That is a diagnostic, not the fix.

## Result

| Observation | KEL-147 |
|---|---:|
| Root AGENTS raw | 12,494 bytes / 2,859 tokens |
| Codex root project instruction item | 12,594 bytes / 2,887 tokens |
| Root tail complete | Yes |
| Local start-message plain-text tokens | 6,960, excluding tool schemas |
| Largest raw automatic chain (root + wv) | 16,209 bytes / 3,818 tokens |
| Wrapped wv project instruction item | 16,326 bytes / 3,852 tokens |
| All four nested markers and root tail | Present |

The automatic project item fell about 63%; the measured local start-message text fell
about 41%. Every nested chain now fits below the 24 KiB repository budget and the Codex
32 KiB default without a local override.

## Representative routed assemblies

These comparable raw-file sums exclude code/spec content and the bounded relevant-area
learning query.

| Task shape | Before | After | Reduction |
|---|---:|---:|---:|
| Root-only | 8,554 | 2,859 | 67% |
| Generic issue implementation floor | 28,837 | 7,028 | 76% |
| Webview + testing | 31,574 | 9,504 | 70% |
| PR review workflow | 30,026 | 9,291 | 69% |
| External research | 10,115 | 4,586 | 55% |

Read-only Codex routing simulations also preserved semantics:

- a CI-router task selected `ci.md` + `testing.md` and retained fail-safe routing and
  required-check rules;
- a macOS webview lifecycle diagnosis selected the nested wv rules + `testing.md`, then
  required independent atoms, real-OS evidence, and a negative control;
- external research selected `research.md`, kept secondary sources as leads, and left
  workflow/review/memory unloaded until their distinct triggers applied;
- a PR/instruction diff selected review, CI, docs, instruction-review, and only the
  changed policy owners; it rejected treating the uncommitted tree as current-tip review;
- a proposed 5 KiB root addition plus budget increase was refused and routed to the
  conditional owner instead.

### Reproduction record

Measurements used these path sets: repository root plus each of
`crates/keld-{compat,guard,ipc,wv}` for automatic traces; root, index, workflow,
task-matched playbooks/skill, and nearest nested AGENTS for routed sums. This executable
trace pins the real user profile, CLI, tokenizer, directories, and output calculation:

```bash
python3 - <<'PY'
import hashlib, json, os, subprocess
from pathlib import Path
import tiktoken

root = Path.cwd()
home = Path.home()
assert subprocess.check_output(["codex", "--version"], text=True).strip() == "codex-cli 0.150.1"
assert tiktoken.__version__ == "0.12.0"
expected = {
    home / ".codex/config.toml": "c5c5796cb03c920bb4b85500b743b098588caed7b884c299491795f37d6bc0cc",
    root / ".codex/config.toml": "22bb2ec446e6c20a0b43359d1899831367f207a1c4d12aabaf1d8473f997563d",
}
for file, digest in expected.items():
    assert hashlib.sha256(file.read_bytes()).hexdigest() == digest, file
assert (home / ".codex/AGENTS.md").read_bytes() == b""

encoding = tiktoken.get_encoding("o200k_base")
root_text = (root / "AGENTS.md").read_text()
for relative in [".", "crates/keld-compat", "crates/keld-guard", "crates/keld-ipc", "crates/keld-wv"]:
    cwd = root / relative
    env = os.environ.copy()
    env["RUST_LOG"] = "error"
    payload = subprocess.check_output(
        ["codex", "debug", "prompt-input", "KEL-147 final trace"], cwd=cwd, env=env
    )
    blocks = [
        block.get("text", "")
        for item in json.loads(payload)
        for block in item.get("content", [])
        if block.get("type") == "input_text"
    ]
    project = next(text for text in blocks if text.startswith("# AGENTS.md instructions for "))
    nested = "" if relative == "." else (cwd / "AGENTS.md").read_text()
    raw = root_text + nested
    print(relative, len(raw.encode()), len(encoding.encode(raw)),
          len(project.encode()), len(encoding.encode(project)), sep="\t")
PY
just agent-context
```

Observed output (`directory`, raw chain bytes/tokens, wrapped item bytes/tokens):

```text
.	12494	2859	12594	2887
crates/keld-compat	13776	3229	13897	3263
crates/keld-guard	14693	3409	14813	3443
crates/keld-ipc	14783	3508	14901	3542
crates/keld-wv	16209	3818	16326	3852
```

Wrapped project-item counts include Codex path/header chrome and therefore vary with
checkout path. Tiktoken counts are estimates, not billing totals.

This executable semantic-eval harness pins the recorded profile hashes above, CLI,
model, reasoning effort, cwd, exact seeds, wall time, and Codex's reported token count:

```bash
#!/usr/bin/env bash
set -euo pipefail
test "$(codex --version)" = "codex-cli 0.150.1"
test "$(shasum -a 256 "$HOME/.codex/config.toml" | cut -d' ' -f1)" = \
  c5c5796cb03c920bb4b85500b743b098588caed7b884c299491795f37d6bc0cc
test "$(shasum -a 256 .codex/config.toml | cut -d' ' -f1)" = \
  22bb2ec446e6c20a0b43359d1899831367f207a1c4d12aabaf1d8473f997563d

run_eval() {
  local label="$1" eval_dir="$2" seed started elapsed output tokens tool_calls
  seed="$(cat)"
  started="$(date +%s)"
  output="$(cd "$eval_dir" && RUST_LOG=error codex exec --ephemeral \
    --sandbox read-only -c 'model="gpt-5.6-sol"' \
    -c 'model_reasoning_effort="low"' "$seed" 2>&1)"
  elapsed=$(( $(date +%s) - started ))
  tokens="$(awk '/^tokens used$/ { getline; print; exit }' <<<"$output")"
  tool_calls="$(grep -c '^exec$' <<<"$output" || true)"
  printf '%s\t%s tool calls\t%s s\t%s tokens\n' \
    "$label" "$tool_calls" "$elapsed" "${tokens:-unreported}"
}

run_eval research . <<'PROMPT'
Hypothetical only; do not edit. I need to research a current external fact for a Keld architecture decision, with no branch or PR work. Inspect repo instructions and state: (1) exactly which routed instruction files apply, (2) which instruction files should not be fully loaded, (3) the evidence order. Keep under 170 words.
PROMPT
run_eval wv crates/keld-wv <<'PROMPT'
Hypothetical only; do not edit. A keld-wv lifecycle test fails on macOS and I need diagnosis, not a CI or PR change. Inspect repo instructions and state: (1) exactly which root/nested/routed instruction files apply, (2) which instruction files should not be fully loaded, (3) the required atomic evidence sequence before selecting a fix. Keep under 190 words.
PROMPT
run_eval pr . <<'PROMPT'
Hypothetical only; do not edit. Review the current feature-branch diff and prepare a PR, but do not perform Linear coordination or external research. Inspect repo instructions and state: (1) exactly which routed instruction files and skill apply, (2) which files should not be fully loaded, (3) the current-head review and PR gates. Keep under 190 words.
PROMPT
run_eval root-budget . <<'PROMPT'
Hypothetical only; do not edit. A contributor proposes adding 5 KiB of conditional policy to root AGENTS.md and raising its budget. Inspect the instruction protocol and decide whether to accept it, naming the required owner and evidence. Keep under 150 words.
PROMPT
run_eval ci . <<'PROMPT'
Hypothetical only; do not edit. A CI-router change is planned. Inspect repo instructions and state the applicable routed files, fail-safe routing contract, required-check behavior, and negative tests without loading unrelated playbooks. Keep under 170 words.
PROMPT
```

| Seed | Working directory | Relevant reads | Unnecessary routed reads / questions | Observed calls, wall time, CLI tokens |
|---|---|---|---|---|
| Current external fact, no branch/PR | root | index + research; local/spec/primary-source order | no workflow/review/memory; none | 2 file reads; 25 s; 18,150 |
| macOS wv lifecycle-test diagnosis | `crates/keld-wv` | nested wv + index + testing + relevant learning/spec slice | no CI/PR/research; none | 2 shell read groups; about 30 s; 31,254 |
| Review current instruction/CI diff and prepare PR | root | changed owners + review/CI/docs/instruction-review/code-review | no memory/external research; none | diff/routed-owner reads; 47 s; 17,505 |
| Add 5 KiB to root and raise its budget | root | index + instructions + manifest + instruction-review | no unrelated playbook; none | 2 shell read groups; parallel batch ≤28 s; 20,791 |
| CI-router edit | root | index + CI + testing + instruction enforcement | no workflow/research/full learnings; none | 2 shell read groups; parallel batch ≤28 s; 14,507 |

The CLI token field includes platform instructions, tool output, and schemas, so it is
reported for transparency but is not the routed-file budget oracle. The local CLI did
not expose monetary cost. No eval asked a clarification question; the larger wv count
came from code/spec/learning evidence returned by tools, not eager policy loading.

## Ownership model

- `always`: root and nested AGENTS; universal/path-local invariants only.
- `routed`: exact `.agents/index.md` trigger or skill description.
- `evidence`: query/slice only; never a mandatory full-file read.

The large learning log remains intact as evidence, but ordinary work queries relevant
areas rather than loading all 16,222 tokens. CI/docs/review/coordination details moved
to separate routed owners. Root retains atomic reasoning, engineering/security floors,
language rules, review gates, and explicit refusal/budget rules.

## Hard enforcement

`.agents/instruction-budget.tsv` inventories every instruction owner/class/trigger and
budget. `tools/agent_context.rs` fails on unknown `.md`/`.txt`, stale, renamed,
discovery-relevant symlinked, comment-only/hollow, or override files; class/trigger drift; duplicate owners; wrong,
missing, non-contiguous, fenced, quoted, HTML, struck, or inline-code routes; mandatory
full evidence reads; per-file overflow; root >16 KiB; nested >4 KiB; or a complete
root-to-directory chain >24 KiB. Manifest caps sit close to current sizes; class ceilings
are backstops, not pre-approved growth allowances.

`just agent-context` runs checker tests and the real checkout. The existing hygiene job
runs the same command; `CI required` makes it merge-blocking. The trigger-only
`instruction-review` skill adds semantic review and prompt-input/eval evidence without
putting its full procedure into every task.

A real negative control removed the visible `instructions.md` route from the index:
`just agent-context` failed on the missing route, then passed after restoration. Checker
fixtures separately pin max+1, unknown/stale/renamed/override, duplicate owner,
class/trigger drift, cumulative nested chains, symlink loops, hollow/comment-only/alias,
wrong-link and Markdown fence/span/table visibility decoys, and mandatory full-evidence
failures.

## Rollback

KEL-147 has no data migration. A full rollback is one revert of its eventual merge
commit, followed by `just atomic-protocol`, `just llms-check`, and prompt traces; this
restores the old root but also restores the proved 32 KiB truncation defect. For a
checker false positive, change the shared Markdown primitive/checker and its negative
control under human review; do not raise an instruction budget or bypass hygiene. A
routed rule can move back only with its consumers, route, manifest row, eval, and token
measurements in the same PR.

## Limitations

- Tool schemas and platform-owned system/developer instructions are outside repository
  byte control and are not included in raw-file sums. Repository `.codex/config.toml`
  currently enables only Linear and Context7 and contains no prompt prose; assembly
  changes are routed to instruction review because config size cannot measure schema
  tokens.
- Installed skill metadata is a separate profile/plugin surface. Remove or profile it
  only after an actual assembly trace proves eager inclusion.
- Prompt caching may reduce latency/cost but cannot repair instruction truncation or
  reduce occupied context.
