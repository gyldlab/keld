# KELD public audit registry

This directory is the canonical public record of KELD technical audits.

Audits are evidence snapshots of pinned source revisions. They describe what the
audited revision demonstrated, contradicted, or left unverified; they are not release
certifications and they do not make future revisions inherit old conclusions.

## Published audits

| Date | Report | Audited KELD revision | Evidence | Status |
|---|---|---|---|---|
| 2026-09-19 | [Full-Spectrum Technical Audit](2026-09-19-full-spectrum-technical-audit.md) | [`0ea0780bb574`](https://github.com/gyldlab/keld/commit/0ea0780bb574ad242e9f1105fa4af5842872bad3) | [manifest](evidence/2026-09-19-full-spectrum-technical-audit.json) | Published |

Original snapshot: `2026-09-19-full-spectrum-technical-audit.md` · SHA-256 `6f4ff0ca989ece1fce0b1627b1ee80e1f91403b6510f4cdf35cb0cd9d8f88ecf`.
Current report: `2026-09-19-full-spectrum-technical-audit.md` · SHA-256 `5197c742180ba4b2e61bd7ef797803162420f2308487c4717e8fc96959fa307c`.
Publication anchor: [`74eb5bb3cd33`](https://github.com/gyldlab/keld/commit/74eb5bb3cd3304be80a3af2d746644a75b315675).
The anchor stores the original report, manifest, and upstream-receipt bytes; an Errata updates only current hashes.
Original manifest: `evidence/2026-09-19-full-spectrum-technical-audit.json` · SHA-256 `cf108bbc2cb408904b2d1253010380c63e6ae8a831f0bf02468d0fc2c14974a6`.
Current manifest: `evidence/2026-09-19-full-spectrum-technical-audit.json` · SHA-256 `1cdf780ed2cb36c2dc546a1ca083eec9357571d924f7006910965949c600df55`.

## Publication contract

- **Pinned scope.** Every report names the exact KELD revision and any separate evidence
  repository revisions that it evaluates.
- **Publicly checkable evidence.** Published technical conclusions must be reproducible
  from public repository history, public benchmark artifacts, public GitHub evidence, or
  authoritative upstream documentation. Private research, private work-management state,
  local machine paths, secrets, and user-specific data are not publication evidence.
- **Historical integrity.** A report is a snapshot. Later implementation changes require
  a new dated audit. Original hashes are permanent, and audit PRs use a history-preserving
  merge so the publication anchor remains reachable from public `main`.
- **Corrections visible to readers.** Published corrections are append-only dated
  `Errata`; original hashes remain, while each correction binds the corrected report and
  semantic evidence state. The maintainer procedure is owned by
  [`.agents/docs.md`](../../.agents/docs.md#public-technical-audits).
- **Uncertainty stays visible.** `Unverified`, `planned`, and `partially demonstrated`
  are valid outcomes. A lack of an established critical finding is not proof that none
  exists.
- **Open work is not shipped capability.** Open pull requests, design documents, and
  planned work may be named as context but are not counted as landed behavior.
- **Machine-readable evidence.** The public verifier checks manifest/report/registry
  agreement for revisions, scope, verification counts, findings, hashes, and corrections.
  Audited repository commits use zlib-compressed loose Git objects; their public commit
  metadata is cryptographic repository evidence and is not treated as private coordination data.
- **Validation.** [`verify.py`](verify.py) is the checker required by the maintainer procedure.

## Severity and confidence

Reports use impact severity and evidence confidence separately.

| Severity | Meaning |
|---|---|
| S0 | Catastrophic or ecosystem-wide impact established by evidence. |
| S1 | Critical security/correctness/release-blocking impact established or strongly supported. |
| S2 | High-impact architectural, security, compatibility, or product-readiness gap. |
| S3 | Material engineering defect, contradiction, reliability issue, or evidence gap. |
| S4 | Minor issue, hygiene problem, or low-impact drift. |

Confidence is **Confirmed** when directly demonstrated by source, test, or artifact
evidence; **High** when the static or contractual evidence is strong but a relevant live
reproduction is absent; and **Unverified** when the audit cannot establish the claim.

## How to cite an audit

Prefer the dated report plus its pinned revision, for example:

> KELD Full-Spectrum Technical Audit, 2026-09-19, audited
> `gyldlab/keld@0ea0780bb574`.

For current product status, use
[`docs/engineering/product-status.md`](../engineering/product-status.md); an audit is
historical evidence, not a replacement for the live status ledger.
