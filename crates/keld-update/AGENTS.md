# keld-update invariants

Extends root `AGENTS.md`. Owner/load: updater/always; trigger: this crate.
Contract: `docs/specs/kel53-full-package-activation.md` T3b (KEL-265).

- Production `unsafe` is limited to `src/windows_fs.rs`: read-only
  `GetVolumeInformationByHandleW`, `GetFinalPathNameByHandleW`, `GetDriveTypeW`;
  directory-only relative `NtCreateFile`, `RtlNtStatusToDosError`, and conversion
  of its successful handle into one RAII owner. MUST use pinned bindings,
  checked live buffers, inline SAFETY proofs and `deny(unsafe_op_in_unsafe_fn)`.
  No generic flags, ABI copies, ambient mkdir or crate-wide allowance.
- The directory adapter accepts one validated component and a retained parent;
  MUST create-new/no-reparse with guard-owned atomic protection and no inherited
  handle or delete sharing. Shared guard owns ACL and package-name policy.
- Extraction receipts MUST preserve installation/key/profile identity. Real root
  descriptors and fixed-NTFS qualification precede writes; logical provenance is
  not OS proof. Retain source, parents and readback handles for their owner lifetime.
- T3b MUST NOT create `.complete`, publish a version or advance floor/pointers.
  Failed writes retain a named incomplete stage, never trusted completion.
- Test-only Win32 mapping/reparse/handle fixtures need local SAFETY proofs.
  Reuse runtime LPAC launch primitives.
  Native failure controls and independent unsafe/security review are required.
  Remove this allowance when these platform operations leave the updater.
