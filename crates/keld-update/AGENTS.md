# keld-update invariants

Root floor applies. Owner/load: updater/always; trigger: this crate.
Contract: KEL-53 T3b/T4a/T4b; KEL-265/266/270.

- Production unsafe MUST stay in src/windows_fs.rs: volume/path, relative NT create/delete/rename (NtSetInformationFile), handle kind/listing, POSIX disposition and reduced-rights activation.lock clone. Pin handles, bound buffers, prove SAFETY.
- Rename: NtCreateFile source (nofollow, FILE_WRITE_THROUGH); NtSetInformationFile destination (one checked name) under SAME retained same-volume parent RootDirectory. No MoveFileExW/ambient/copy fallback or caller paths/flags. publish_new absent-only; typed replace journal/floor/current/LKG/previous-known-good only under writer lease after pending sibling flush/readback.
- Relative create: one validated component/retained parent, create-new/no-reparse, guard-owned ACL/name policy, no inherited handles/delete sharing.
- Preserve install/key/profile identity. Check actual descriptors and fixed NTFS before writes; logical provenance is not OS proof. Retain source/ancestor/readback handles for owner lifetime.
- T3b never creates .complete, publishes versions or advances floor/pointers. T4a alone seeds the baseline as actual SYSTEM, with persistent ancestry and provenance last; failure stays incomplete.
- Mapping/reparse/handle fixtures need local SAFETY; reuse runtime LPAC. Require OS failure controls and independent unsafe/security review. Retire allowances when operations leave updater.
