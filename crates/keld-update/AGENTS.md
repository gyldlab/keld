# keld-update invariants

Root floor applies. Owner/load: updater/always; trigger: this crate.
Contract: KEL-53 T3b/T4a/T4b; KEL-265/266/270.

- Production unsafe MUST stay in src/windows_fs.rs: volume/path queries, relative NT creation/RAII and reduced-rights activation.lock duplication. MoveFileExW is same-parent WRITE_THROUGH: publish_new stays absent-target; typed record replacement is only journal/floor/current/LKG/previous-known-good, REPLACE_EXISTING | WRITE_THROUGH under the stable lease after protected-sibling flush/readback. No caller paths/flags, copy or ambient mkdir. Use pinned bindings, checked buffers, local SAFETY proofs and unsafe-op denial.
- Relative creation takes one validated component and retained parent, create-new/no-reparse, guard-owned profile, no inherited handles/delete share. Guard owns ACL/name policy.
- Preserve install/key/profile identity. Check actual descriptors and fixed NTFS before writes; logical provenance is not OS proof. Retain source/ancestor/readback handles for owner lifetime.
- T3b never creates .complete, publishes versions or advances floor/pointers. T4a alone seeds the baseline as actual SYSTEM, with persistent ancestry and provenance last; failure stays incomplete.
- Mapping/reparse/handle fixtures need local SAFETY; reuse runtime LPAC. Require OS failure controls and independent unsafe/security review. Retire allowances when operations leave updater.
