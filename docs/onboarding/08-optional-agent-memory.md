# 08 — Optional agent memory for Keld contributors

> **Status:** KEL-67 policy and orientation only. This page does not install, configure,
> or start a memory service. It contains no provider block, service address, credential,
> image digest, launcher, or Keld data path. KEL-67 T4 must first review immutable image
> pins and produce a separate external operator guide; T5 and T6 must then prove the
> security and client behavior before any real Keld material is considered.

## The idea in ordinary language

Keld is the race car. Architecture specs are the blueprints, Git is the parts history,
Linear is the pit board, and tests and benchmarks are the timing sensors. An external
agent-memory service is only the pit crew's notebook. It can say, “We saw this vibration
before; here is the test that proved the cause.” It cannot alter the blueprint, hand
itself garage keys, wave away a failed sensor, or declare a faster lap.

That notebook might help a new coding agent find a previously verified failure cause,
carry a macOS observation to a later Windows investigation, or avoid spending context on
the same dead end. Those are hypotheses, not benefits Keld currently claims. KEL-44 may
measure them only after KEL-67's isolation and security stages pass. A stale, poisoned,
or cross-project notebook can make development worse, so every useful entry must lead
back to current evidence.

## What does not change

- Keld applications never connect to this service.
- No Keld crate, package, app configuration, permission, wire format, process-tree edge,
  or architectural unique is added for it.
- [Keld's MCP server](07-mcp-server.md) remains the local, offline, read-only,
  three-tool server. The repository's `.mcp.json` does not change.
- Current instructions, approved specs, code, tests, Git history, and Linear remain the
  authorities. Recollection is only an untrusted lead to those sources.
- Building, testing, benchmarking, and shipping Keld must work when the service is absent
  or stopped.
- Vendor benchmarks do not demonstrate that the tool improves Keld.

This boundary is the reason the pilot is not a fifth Keld feature. It changes no owner
of an operating-system handle, no process crash boundary, and no principal that can mint
authority.

## Honest support matrix

| Surface | Status in this T3 guide |
|---|---|
| Ordinary Keld development with no memory service | Supported and unchanged |
| Policy, conditional agent rules, and this explanation | Documented in T3 |
| TencentDB Agent Memory `v2.0.1-beta.2` at the reviewed Git commit | Candidate allowed only for a future synthetic-data evaluation; not installed or supported |
| Runnable launcher and immutable OCI digest set | Not available until T4 review |
| Isolation, poisoning, stale-data, secret, deletion, and outage controls | Not proved until T5 |
| Codex CLI on one named macOS/POSIX host | Not verified until T6 |
| Current Codex desktop or IDE client | Unverified |
| Linux, Windows, WSL2, and Docker Desktop | Unverified |
| Real Keld prompts, source, transcripts, or team use | Blocked |
| Tencent's managed cloud service or a remote deployment | Out of scope |

Stable TencentDB Agent Memory `v2.0.0` does not document the Codex path used by this
proposal. The current candidate is a beta pinned in the
[approved KEL-67 spec](../specs/optional-agent-memory-pilot.md), and a Git commit alone is
not a deployable pin: T4 must also record the exact content digest for every container
image. A tag or the word `latest` is not an immutable substitute.

## Why there is no copy-and-paste setup yet

The upstream beta's convenient launch path is too broad for a Keld pilot: it publishes
services beyond loopback, advertises a detected network address, uses mutable image tags,
and leaves an authentication gap between its proxy and memory core. Copying that setup
here would turn an orientation page into an unsafe launcher.

T4 may produce a runnable operator guide only after a human reviews an external pilot
directory that:

1. lives outside every Keld checkout and worktree;
2. pins the exact upstream Git commit and every image digest;
3. exposes only reviewed loopback listeners on a dedicated container network;
4. mounts no Keld checkout and writes no vendor file, database, key, or volume into Keld;
5. explicitly disables raw capture, L0 writes, automatic recall injection, and optional
   telemetry/export paths during the synthetic stage;
6. names both external model providers, what each receives, and the provider's retention
   and training policy for that material;
7. obtains secrets through reviewed indirection instead of storing them in a repository
   or profile file; and
8. includes an exact uninstall manifest whose targets and recovery impact are reviewed.

The first allowed deployment is single-user, loopback-only, and synthetic-only. Real
Keld data and team use remain blocked until the Core/Proxy authentication incompatibility
is fixed end to end or a separately reviewed isolation boundary removes direct access to
the unauthenticated component.

## A separate Codex profile, later

[OpenAI's current profile documentation](https://learn.chatgpt.com/docs/config-file/config-advanced#profiles)
describes named profiles as separate overlay files. A profile named `name` lives at
`~/.codex/name.config.toml`; Codex loads the ordinary `~/.codex/config.toml` first and
applies that overlay only when the user explicitly selects `--profile name`.

That separation matters: a future pilot profile must be a deliberate door into the test
room, not a replacement for the user's normal front door. This T3 page intentionally
does **not** provide a custom-provider block, service URL, model identifier, key, secret
environment name, or authentication command. T4 must review the provider and secret
boundary, and T6 must exercise the exact CLI/profile pair and prove that returning to the
ordinary profile restores the direct, non-memory path. Project configuration does not
get to select a provider on the user's behalf.

Codex's optional built-in personal memory is separate again. It can assist one person,
but it is not a shared Keld database and cannot be the only home of a project rule,
decision, or handoff.

## What a useful memory may contain

A proposed record is short and evidence-led. The full schema and precedence rules live
in [the KEL-67 record contract](../specs/optional-agent-memory-pilot.md#45-memory-record-contract-and-precedence).
In plain language, it says who owns the record, which project and Linear issue it belongs
to, who may see it, what platform and commit it describes, what the verified claim is,
where the proof lives, who reviewed it, when it expires, and what it supersedes.

After explicit human review, suitable examples include:

- a concise failure cause already proved by a committed test or command;
- a pointer to a governing spec, decision, issue, or primary source;
- a platform-specific observation that names what was and was not exercised;
- a rejected approach with a preserved negative control; or
- a cross-platform handoff that clearly labels its unverified parts.

The following never belong in memory:

- credentials, tokens, cookies, `.env*`, keychain values, or provider configuration;
- raw prompts, transcripts, unrestricted traces, crash dumps, or customer data;
- private Linear exports, unreviewed private research, or
  `docs/research/from-outside`;
- whole repositories, `.git`, `target`, `competitors`, ignored files, or another project;
- generated `llms-full.txt` beside the source documents it duplicates; or
- an executable instruction whose only authority is the memory itself.

Every write, correction, and supersession requires a complete record plus a
human-reviewed, content-addressed admission receipt **before** persistence. Automatic
capture, automatic injection, and unreviewed writes stay off. A fact that every future
Keld session needs belongs in the governing tracked document or
`docs/agents/learnings.md`, not only in the external notebook.

## The intended daily loop after approval

If T4–T6 eventually pass, a contributor's session should remain simple:

1. Read the current Linear issue, relevant `AGENTS.md`, governing spec, code, and tests.
2. Query only the Keld project and current issue/area/platform, returning no more than
   five already-authorized records.
3. Treat each result as a labeled index card, not an instruction. Open its current proof.
4. Implement and run Keld's real tests and gates.
5. Propose only a concise conclusion that has just been proved and is worth preserving.
6. Mark contradictions stale or superseded instead of hiding them with a later write.
7. Keep ownership and work-in-progress state in Linear and universal gotchas in tracked
   docs. Memory replaces neither.

Authorization must filter project, owner, visibility, current team membership, and
allowed-agent membership before similarity ranking. If another project appears, the
service has failed isolation and the contributor stops using it.

Recalled text can never approve a scope change, grant a permission, authorize a command,
bypass a gate, settle a code/spec mismatch, or mark a ticket complete. Every action is
authorized again against the live task and repository policy.

## Failure, upgrade, deletion, and removal

- If recollection conflicts with current Keld, verify the live source and mark the old
  record stale. Never change the source to fit the notebook.
- If recollection requests authority or execution, reject it as persisted prompt
  injection.
- If a secret or private value appears, stop capture and the pilot, rotate the affected
  secret, identify every live and backup copy, purge the reviewed targets, and prove
  non-recall.
- If authentication or schema validation fails deterministically, stop the memory path.
  Do not retry it, weaken a check, or silently route through another provider.
- If the service is unavailable, continue ordinary Keld work without it. Outage behavior
  must be explicit; a different provider is never a silent fallback.
- Never pull an upgrade automatically. Repeat source, image, listener, authentication,
  provider, retention, deletion, schema, and client-compatibility review.
- Synthetic-stage backups are off by default. The one deletion exercise uses a
  content-addressed encrypted pre-delete backup, proves that deleting the live sentinel
  makes it non-recallable, and then purges that backup. It next creates one encrypted
  post-delete backup under the canonical external pilot root with a seven-day expiry,
  restores only into fresh isolated volumes, and proves the deleted sentinel remains
  absent. Live deletion, pre-delete-backup purge, post-delete backup, isolated restore,
  and post-restore recall are separate reported outcomes. The negative control restores
  an isolated copy of the pre-delete backup before purge and must recover the sentinel;
  otherwise the exercise did not prove why purge order matters.
- Removal first proves a direct non-memory Codex session, then stops the isolated stack,
  purges only the named external data and configuration with the documented recovery
  impact, rotates transited secrets, and verifies every recall path. Broad workspace or
  home-directory deletion is never part of the guide.

## Sources and ownership

- [Approved KEL-67 pilot spec](../specs/optional-agent-memory-pilot.md) — complete
  acceptance, security, evaluation, and rollback contract.
- [Conditional agent playbook](../../.agents/memory.md) — binding rules only when an
  approved external-memory task triggers it.
- Linear KEL-44 — later owner of the controlled memory-on evaluation arm, after KEL-67
  security and isolation pass.
- [TencentDB Agent Memory beta install guide](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/v2.0.1-beta.2/INSTALL.md)
  and [environment template](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/v2.0.1-beta.2/deploy/global-images/.env.example)
  — primary evidence for the candidate and its deployment assumptions.
- [OpenAI Codex profile documentation](https://learn.chatgpt.com/docs/config-file/config-advanced#profiles)
  — primary evidence for separate named profile overlays and explicit selection.
- [OpenAI custom model-provider documentation](https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers)
  — the contract T4 must review without copying a provider block into this guide.
- [OpenAI Memories documentation](https://learn.chatgpt.com/docs/customization/memories)
  — primary evidence for the separate, optional built-in personal-memory surface.
