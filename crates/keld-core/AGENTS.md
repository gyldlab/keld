# keld-core invariants

Extends root AGENTS.md for Host sessions.
- `app_session.rs` owns no-flag lifecycle/routing. Reuse Runtime/WebView primitives. MUST NOT duplicate generation, Job, LPAC, ACL, Guard or wire policy.
- Windows production unsafe: reviewed lease-pipe state/flags and cleanup-sentinel process acquire/image/wait in `app_session.rs`. Guard owns KEL-135 Authenticode (KEL-270 D4).
- Windows facts unsafe only in `src/app_session/launch_facts/windows_native.rs`: GetUserPreferredUILanguages, no-create SHGetKnownFolderPath (RoamingAppData/Documents), bounded reads, owned CoTaskMemFree. Own output before entry. No shutdown lock.
- macOS KEL-135/T3 unsafe only in `src/macos_profile_identity.rs`: SecCodeCopySelf → SecRequirementCreateWithString → SecCodeCheckValidity → SecCodeCopyStaticCode → SecCodeCopySigningInformation, then owned CF-reference conversion. Current process only. Identity fields require Apple-chain validation. API/missing fact: KELD-WV-009. No arbitrary code/path or blanket unsafe.
- Every unsafe block needs local `// SAFETY:`. Windows calls require independent unsafe/security review before build.
- Startup is resource-free until validated boot and immutable Guard preflight pass. Keep revocation, link close, child reap, window exit and cleanup errors ordered and independently observable.
- Pre-attach revocation, accepted shutdown, stale generation or missing owner MUST NOT provision a successor. Transition changes require real-platform tests and a mutation/negative control.
