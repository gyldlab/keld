# KEL-270 D90: single repository writer grant candidate

**Status: REVIEW CANDIDATE ONLY. No native grant, D90 witness, installer authority, or release qualification.**

Source base: published `7f48875e82caea45e596fada4a3f387f220fbe0f`
on `origin/agent/kel-270-native-production-join-20261121`.
Device: `DESKTOP-3FMM79E`, 2026-11-23. This candidate does not modify
`AGENTS.md`, grant a new Windows FFI, or enable the shipping Host role.

## Decision-bearing ownership facts

| Atom | Current source and owner | Missing fact and first falsifier |
| --- | --- | --- |
| One repository identity | D44: `LocalAppData/GYLDLAB/Keld/acquisition-v1/repositories/<repository-id>` (Windows spec). `repository_metadata.rs:80-139` borrows one directory set. | No authenticated D42 known-folder + retained D44 root constructor exists. Two different physical repository directories can each admit independent file locks. |
| One exclusive writer | `windows_baseline.rs:656-686` opens only an **installation** `activation.lock`. `repository_census.rs:73-91` has an uninhabited D90 witness. | No repository-wide lease object/name or creation owner is qualified. Two installations with one app/repo must block on the same retained repository object. |
| D64 phase transition | `initial_admission_journal.rs:273-379` publishes initial admission/pointer but cannot replace an existing admitted journal phase. | The native owner must bind a generated pending file to the recorded attempt and exact previous phase, then durably replace only that journal. |
| D68 terminal removal | `repository_metadata.rs` and `initial_admission_journal.rs` read the pending pointer but provide no qualified terminal retirement owner. | The same lease and exact terminal proof must own one pinned pointer handle through deletion and absent readback. |
| D46 first effect | `initial_permit.rs:35-69,108-173` holds typed issuer/census/revision borrows. The issuer is unimplemented. | A caller-supplied path, empty scaffold or journal cannot substitute for D42 worker, D43 creator and T20/D90 live proof. |
| Old readers | Integration D90 requires all supported v1/v2 Host/Guard/launcher versions and admitted launch drain. | Unknown predecessor, live old launch, missing seal or lost seal after restart must refuse any trust switch. |

## Exact minimum native-owner decision proposed for T82 review

1. **Name one canonical lock file.** Candidate leaf: `repository.lock`, directly
   below the already authenticated D44 repository directory. This name is
   **not yet specified or approved**. The existing updater owner and council must
   choose and pin it once. No per-installation, per-channel or alternate lock.
2. **Provision once under the same D44 owner.** After native D42/D43 Direct
   discovery and authenticated retained-parent/SID checks, the owner may issue
   a separate, reviewed, create-new regular-file operation. No installation
   tree may be created by lock provisioning. An existing unknown/corrupt lock
   is not automatically adopted, recreated or deleted.
3. **Open the persistent lease through the retained repository handle.**
   Proposed `NtCreateFile`: one fixed `UNICODE_STRING` leaf; `OBJECT_ATTRIBUTES`
   `RootDirectory` = the held repository directory;
   `OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE`;
   disposition `FILE_OPEN`, no create/overwrite;
   options `FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT |
   FILE_SYNCHRONOUS_IO_NONALERT`; **desired access
   `FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE`
   = `0x00120081`, share mode `0`**. Retain this exact owned handle
   until the bound operation ends. Do not use an ambient path or reconnect
   after a conflicting open.
4. **Validate the live object.** Require one regular, non-reparse, zero-byte
   file, one link, expected native volume and exact Guard
   `PerUserOwnerPrivate` descriptor. Verify the directory's retained
   ancestry, app/repository/SID association, policy, and actual worker.
   Repeat live object and descriptor checks before each protected effect.
   Share mode alone does not prevent an attributes-only reopen or make
   reparse metadata immutable.
5. **Use only one KEL53 owner.** Its private noncloneable lease must be retained
   across D90 census, D64 admission, D68 pointer, D46 first effect and
   post-cut readback. All enrollment, ordinary refresh, relocation, retirement
   and old-reader admission must use this one object. A direct installation
   `activation.lock` is a later, separately scoped lock and never a substitute.
6. **Keep the authority witness closed.** A live lease alone is not
   `RepositoryLedgerCensus`, `ReadyRevision` or an initial-install permit.
   The D90 producer must independently prove complete historical v1/v2
   namespace and permit coverage, T81 channel, known reader capabilities,
   admission seal, and process/browser drain. Restart must recreate seal/drain
   evidence under the same lease.

Two further rights require their **own independent KEL53 owner grant**:

- **D64 replacement:** under that same retained lease, accept only the
  recorded `admissions/<attempt-id>.json` phase target and a flushed/read-back
  generated `pending-*` regular source in the **same retained admissions
  parent**. Use the existing native `NtCreateFile` no-follow,
  `FILE_WRITE_THROUGH` source open and `NtSetInformationFile` with
  `FileRenameInformation`. Request only `DELETE | FILE_READ_ATTRIBUTES |
  READ_CONTROL | SYNCHRONIZE` (and its audited required rights), share
  `FILE_SHARE_READ`. Bind prior bytes, exact attempt, phase, root and owner
  before replacement and verify after. No general replacement target API.
- **D68 retirement:** take an exact `pending-operation.json` handle from
  the same retained root with `FILE_OPEN | FILE_OPEN_REPARSE_POINT`.
  Request `FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | DELETE |
  SYNCHRONIZE`, share `FILE_SHARE_READ`. First prove a fully matching
  `complete` or `aborted` D64 record, unchanged required current/LKG state,
  no-effect proof when needed, and exact owner/attempt. Only then allow the
  existing `FileDispositionInfoEx` POSIX delete on the **same pinned handle**,
  followed by an absent-pointer readback. Never delete by name, retry a
  conflict, or clear a pointer from a directory scan.

These proposed rights require implementation-specific independent review.
They are not an update to the crate's unsafe allowlist.
The D43 `ExactEmptyScaffold` creator proof is separately uncommissioned.
`FreshAbsent` must publish/reread D64 `creation-armed` and D68 before
the first installation-specific creation. An orphan remains a refusal.

## Independent Windows evidence and falsifiers

Run the disposable oracle:

```powershell
powershell.exe -NoProfile -NonInteractive -File docs/engineering/evidence/kel270-catalogue-admission-v2/verify_d90_repository_lease_rights.ps1
```

Rushil observation, 2026-11-23, Windows PowerShell 5.1:

| Real handle observation | Output | Why it matters |
| --- | --- | --- |
| `0x00120080`, share 0, followed by data-read open | `D90_LOW_RIGHTS_FALSE_EXCLUSION=PASS` | Data-read open **succeeds**. A metadata-only zero-share handle is not the proposed writer lease. |
| `0x00120081`, share 0, followed by data-read open | `D90_SAME_FILE_EXCLUSIVE=PASS ... errno=32` | A data-access owner handle excludes a second data reader of the same file object. |
| Same fixed leaf beneath different repository root | `D90_DIFFERENT_REPOSITORY_STILL_OPENS=PASS` | File sharing cannot prove D44's correct app/repo/root selection. |
| Concurrent attributes-only opener while data lease held | `D90_ATTRIBUTE_ONLY_REMAINING_RISK=PASS` | Reparse/attribute state is not frozen by share mode. |
| Release/reacquire and absent lock with `FileMode.Open` | `D90_REACQUIRE_AND_MISSING_LOCK=PASS` | Handle lifetime matters, and a missing lock does not authorize self-creation. |

The second reader itself shares reads and writes. This isolates the
first owner's share exclusion rather than failing on the second reader's
incompatible sharing declaration.

This probe uses real Windows `FileStream` handles and file-rights masks.
It does not exercise `NtCreateFile` relative-parent options, Guard DACL
validation, separate processes, physical crash cuts, or actual production
publication. Those cells remain **NOT RUN**, not inferred PASS.

The test is deliberately sensitive to two changes: removing
`FILE_READ_DATA` from the proposed owner makes the second-open failure
assertion RED; replacing the canonical repository root with another physical
root leaves both locks open and defeats repository-wide exclusion.

Verified temporary mutations on Rushil (original files unchanged):

- Removing `FILE_READ_DATA` (and adjusting only the expected mask) produced
  `D90_MUTANT_RED=remove-FILE_READ_DATA exit=1` at the OS reader oracle.
- Replacing the owner's `FileShare.None` with `FileShare.ReadWrite` produced
  `D90_MUTANT_RED=remove-owner-share-zero exit=1` at the same oracle.

These checks test local Win32 sharing behavior. They do not certify native
reparse invariance, independent process races or the repository-wide D90
source-reader/permit census. The user's Windows MVP may use local development
signing; commercial signing and public release remain separate work.

Microsoft primary sources, checked 2026-11-23:

- [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew):
  attribute/extended-attribute access requests are outside share-mode exclusion.
- [MS-FSA FSCTL_SET_REPARSE_POINT](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/4aeefef8-92c3-4abc-af7a-a610caf8a165):
  the operation accepts `FILE_WRITE_DATA` **or** `FILE_WRITE_ATTRIBUTES` access.
- [Reparse point operations](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-operations):
  operations act on the supplied file handle and require explicit no-follow handling.

Next owner action: independently review the proposed fixed leaf, provisioning
time, exact rights and retained-handle binding. Then update the **existing**
`crates/keld-update/AGENTS.md` under its 4 KiB budget, with mandatory instruction
review and exact native tests. Only after that approval may source implementation
add this lease and D64/D68 effects. T20, D42/D43, D90 old-reader drain and
T81 development-signing qualification remain separate gates.
