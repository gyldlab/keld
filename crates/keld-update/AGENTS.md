# keld-update invariants

Extends root AGENTS.md. Owner/load: updater/always; trigger: this crate.
Contract: docs/specs/kel53-full-package-activation.md T3b/T4a (KEL-265/266).

- Production unsafe only in src/windows_fs.rs: read-only
  GetVolumeInformationByHandleW, GetFinalPathNameByHandleW, GetDriveTypeW;
  directory-only relative NtCreateFile, RtlNtStatusToDosError and successful
  handle RAII; MoveFileExW only absent-target same-parent publication with
  MOVEFILE_WRITE_THROUGH. No copy/replace, generic ABI/flags or ambient mkdir.
  Use pinned bindings, checked live buffers, local SAFETY proofs and
  deny(unsafe_op_in_unsafe_fn).
- Relative creation takes one validated component and retained parent, create-new/
  no-reparse with guard-owned atomic protection, no handle inheritance or delete sharing.
  Guard owns ACL and package-name policy.
- Preserve installation/key/profile identity. Validate actual descriptors and fixed
  NTFS before writes; logical provenance is not OS proof. Retain source, ancestors
  and readback handles for owner lifetime.
- T3b never creates .complete, publishes versions or advances floor/pointers.
  T4a alone initializes baseline; actual SYSTEM, persistent ancestry and provenance
  last are required. Failure leaves incomplete state, never completion.
- Test Win32 mapping/reparse/handle fixtures need local SAFETY proofs; reuse runtime
  LPAC launch. Native failure controls and independent unsafe/security review required.
  Retire allowance when these operations leave updater.
