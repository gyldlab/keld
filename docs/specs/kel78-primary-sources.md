# KEL-78 primary-source ledger

Linear: [KEL-78](https://linear.app/gyldlab-keld/issue/KEL-78/spec-strict-profile-os-sandbox-for-bun-roles-and-native-addon-workers)
Status: draft ledger (not a containment proof)
Access date for every fetch in this file: 2026-08-19
Base SHA this ledger was written against: `67f39cdc898254f1e0c9cd50800f242ae7a4c493` (`origin/main`)

This file is the citable authority list for
[`kel78-strict-profile-sandbox.md`](kel78-strict-profile-sandbox.md). A URL or
man page here is evidence of an OS contract. It is **not** evidence that Keld
applies that contract. Every platform remains **unverified** until a hostile
probe against a shipped artifact is archived.

Do not treat Chromium blogs, forum posts, competitor sandboxes, or
`docs/research/` notes as proof. Those may be leads only.

## How to read a row

| Column | Meaning |
|---|---|
| Source | Direct official URL or local man page |
| Publisher | Owner of the page |
| Dated | Page `ms.date`, man-pages colophon, or man page footer |
| Quote | Exact supporting passage |
| Use | What the KEL-78 spec may claim from this source |

Rows marked **fetch-limited** were not fully readable as rendered HTML in this
pass (Apple Developer pages that require JavaScript). The URL remains the
official locator. **Quote stays empty** — do not invent, paraphrase, or paste
forum or search-engine text into Quote. Fail closed on any claim that would
need that missing sentence.

---

## Cross-cutting local code (not an OS proof)

These are facts about this repository at `67f39cdc`. They prove the *absence*
of a sandbox, not containment.

| Fact | Evidence |
|---|---|
| `keld-runtime` supervises one child with spawn, stdout/stderr pipes, backoff, and crash-loop breaking. It does not apply an OS sandbox. | `crates/keld-runtime/src/lib.rs` crate docs and `spawn_piped` |
| `spawn_piped` sets `stdout`/`stderr` to pipes and then `Command::spawn`. It does not close other inherited descriptors or set a handle allowlist. | `crates/keld-runtime/src/lib.rs` `spawn_piped` |
| `keld-guard` principals are `AppProcess`, `Webview`, and `Plugin`. There is no addon-worker principal. | `crates/keld-guard/src/lib.rs` `Principal` |
| Architecture 03 §4.2 still names a progressive `sandbox_init` / restricted-token / landlock+seccomp sketch. That sketch is destination prose, not a passed proof. | `docs/architecture/03-security.md` §4 |
| KEL-75 already requires fail-closed admission when a strict profile cannot be applied, and assigns real-OS proof to KEL-78. | `docs/specs/kel75-principalized-bun-child-roles.md` AC6, T6 |

---

## macOS

### M1. `sandbox_init(3)` is deprecated; App Sandbox is the replacement

- **Source:** local man page `sandbox_init(3)` (`SANDBOX_INIT(3)`, Mac OS X, March 9, 2017)
- **Publisher:** Apple (Darwin man pages)
- **Dated:** March 9, 2017
- **Quote:** "The sandbox_init() and sandbox_free_error() functions are DEPRECATED. Developers who wish to sandbox an app should instead adopt the App Sandbox feature described in the App Sandbox Design Guide."
- **Use:** Architecture 03 §4.2's `sandbox_init` target is not the current Apple contract. KEL-78's macOS candidate is App Sandbox. Calling `sandbox_init` is not a containment proof.

### M2. Enabling App Sandbox removes most capabilities; entitlements restore them

- **Source:** <https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html>
- **Publisher:** Apple Developer Documentation Archive
- **Dated:** archive page (Entitlement Key Reference); accessed 2026-08-19
- **Quote:** "You can think of using App Sandbox entitlements as a two-step process: Sandbox a target, which removes most capabilities for interacting with the system. Restore capabilities to the sandboxed target, as needed, by configuring App Sandbox entitlements."
- **Use:** Strict profile starts at `com.apple.security.app-sandbox` with no extra capability entitlements unless a recorded experiment proves they are required for Bun to start.

### M3. The App Sandbox entitlement key

- **Source:** same as M2
- **Quote:** "`com.apple.security.app-sandbox` — Enables App Sandbox for a target in an Xcode project"
- **Use:** Admission must observe this entitlement on the launched Bun/helper binary. Absence is fail-closed for the strict state.

### M4. Network, files, devices, and personal-information entitlements are explicit grants

- **Source:** same as M2
- **Quote:** "`com.apple.security.network.client` — Network socket for connecting to other machines"; "`com.apple.security.network.server` — Network socket for listening for incoming connections initiated by other machines"; "`com.apple.security.files.user-selected.read-write` — Read/write access to files the user has selected using an Open or Save dialog"; "`com.apple.security.device.camera` / `com.apple.security.device.audio-input`"; "`com.apple.security.personal-information.addressbook`"
- **Use:** A strict Bun role does not receive client/server network, user-selected files, device, or personal-information entitlements by default. Host Powerbox stays on the host.

### M5. Child inheritance is a two-key-only profile

- **Source:** same as M2, section "Enabling App Sandbox Inheritance"
- **Quote:** "To enable sandbox inheritance, a child target must use exactly two App Sandbox entitlement keys: `com.apple.security.app-sandbox` and `com.apple.security.inherit`. If you specify any other App Sandbox entitlement, the system aborts the child process." Also: "using a child process does not provide the security afforded by using an XPC service."
- **Use:** Inherit copies the **parent** sandbox. `keld-host` is the privileged TCB, so a Bun helper MUST NOT inherit from the host. Extra entitlements on an inherit child abort it. Narrow XPC (M6) is the documented privilege-separation mechanism; a separately signed helper with its own App Sandbox is the KEL-78 launch path.

### M6. XPC services get their own sandbox; posix_spawn/NSTask do not

- **Source:** <https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html>
- **Publisher:** Apple Developer Documentation Archive
- **Dated:** archive "Creating XPC Services"; accessed 2026-08-19
- **Quote:** "Other mechanisms for dividing an application into smaller parts, such as NSTask and posix_spawn, do not let you put each part of the application in its own sandbox, so it is not possible to use them to implement privilege separation. Each XPC service has its own sandbox, so XPC services can make it easier to implement proper privilege separation." Also: "By default, XPC services are run in the most restricted environment possible—sandboxed with minimal filesystem access, network access, and so on. Elevating a service’s privileges to root is not supported. Further, an XPC service is private, and is available only to the main application that contains it."
- **Use:** Host-owned XPC helpers (if any) are private to the host bundle, default-deny, and never root. A Bun `spawn`/`NSTask` child is not an XPC service.

### M7. Security-scoped bookmarks / Powerbox persistence are entitlement-gated

- **Source:** same as M2, "Enabling Security-Scoped Bookmark and URL Access"
- **Quote:** "If you want to provide your sandboxed app with persistent access to file system resources, you must enable security-scoped bookmark and URL access." Keys: `com.apple.security.files.bookmarks.app-scope`, `com.apple.security.files.bookmarks.document-scope`.
- **Companion (official locator, fetch-limited):** <https://developer.apple.com/documentation/foundation/nsurl/startaccessingsecurityscopedresource()>
- **Companion Quote:** not captured. This fetch returned a JavaScript shell ("This page requires JavaScript"), not the rendered documentation body. Forum posts, search-engine summaries, and entitlement-catalog siblings are **not** this page's Quote. Fail closed the same way as M8: do not invent an Apple sentence.
- **Use:** Bun roles do not receive bookmark entitlements. Powerbox and security-scoped grants, if used at all, stay on the host. The archive entitlement Quote (M2 keys) remains the citable M7 body.

### M8. Hardened Runtime is a signing runtime, not App Sandbox

- **Source (official locator, fetch-limited):** <https://developer.apple.com/documentation/security/hardened_runtime>
- **Publisher:** Apple
- **Dated:** current Apple documentation; accessed 2026-08-19; re-fetched the same day and still a JavaScript shell
- **Quote:** not captured. This fetch returned a JavaScript shell ("This page requires JavaScript"), not the rendered documentation body. Forum posts, search-engine summaries, and entitlement-catalog siblings are **not** this page's Quote.
- **Use:** Fail closed: Hardened Runtime alone does **not** admit the strict state, including while Quote is empty. App Sandbox remains the required macOS authority boundary (M2–M5). JIT-related entitlements (`com.apple.security.cs.allow-jit` and siblings) remain experimental minima only; each must be forced by a recorded Bun-start failure. A later ledger pass MAY fill Quote from the rendered Apple page; filling Quote is not a containment proof.

---

## Windows

### W1. AppContainer isolates credentials, device, files, network, process, and windows

- **Source:** <https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation>
- **Publisher:** Microsoft Learn
- **Dated:** `ms.date` 2025-07-08
- **Quote:** "Isolation is the primary goal of an AppContainer execution environment. By isolating an application from unneeded resources and other applications, opportunities for malicious manipulation are minimized." Also: "The AppContainer environment creates an identifier that uses the combined identities of the user and the application, so credentials are unique to each user/application pairing and the application cannot impersonate the user."
- **Use:** Ordinary AppContainer is a sandbox *family*, not the Keld strict profile. Credential impersonation of the interactive user is the threat this identity pairing addresses.

### W2. Regular AppContainer still has default access that LPAC does not

- **Source:** <https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer>
- **Publisher:** Microsoft Learn
- **Dated:** `ms.date` 2023-07-20; page updated 2025-09-03
- **Quote:** "Regular AppContainers are granted access to certain system files/directories, common registry keys and COM objects, however, LPAC needs specific capabilities to access resources that regular AppContainers can access. Less Privileged AppContainers (LPAC) are even more isolated than regular AppContainers and require further capabilities to gain access to resources that regular AppContainers already have access to such as the registry, files, and others. For example, LPAC cannot open any keys in the registry unless it has the registryRead capability and cannot use COM unless it has the lpacCom capability."
- **Use:** Ordinary AppContainer, MSIX packaging, Low IL, or a restricted token alone cannot admit the strict state. The candidate is LPAC.

### W3. LPAC launch is an explicit All-Application-Packages opt-out

- **Source:** same as W2
- **Quote:** "The following example shows how to launch a less privileged AppContainer (LPAC), which requires an additional process/thread attribute" using `PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT` via `PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY`. Also: "This will include the package SID, capabilities, if any, as well as if this should be an LPAC, which is specified by opting out of All Application Packages."
- **Use:** Strict admission constructs LPAC explicitly. A zero-capability LPAC means `CapabilityCount = 0` **and** the All Application Packages opt-out. Missing the opt-out is a regular AppContainer.

### W4. Dual-principal DACL: user SID ∩ package/capability SID

- **Source:** same as W2
- **Quote:** "the permitted access is the intersection of that granted by the user/group SIDs and AppContainer SIDs so if the User has full access, but the AppContainer only has read access, the AppContainer can only be granted read access."
- **Use:** Runtime and data ACLs must name the package SID (and any reviewed capability SID). A DACL that allows the user but not the package SID is the intended deny. Tests must read the real DACL, not assume "AppContainer" implies deny.

### W5. Job objects manage descendant process trees; they are not the token sandbox

- **Source:** <https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects>
- **Publisher:** Microsoft Learn
- **Dated:** `ms.date` 2025-07-14
- **Quote:** "A job object allows groups of processes to be managed as a unit." Also: "After a process is associated with a job, by default any child processes it creates using CreateProcess are also associated with the job." And: "To terminate all processes currently associated with a job object, use the TerminateJobObject function." And: "if the job has the JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE flag specified, closing the last job object handle terminates all associated processes."
- **Use:** A job is required for descendant kill/accounting. It does not replace LPAC. `JOB_OBJECT_LIMIT_BREAKAWAY_OK` / `SILENT_BREAKAWAY_OK` are forbidden on the strict job.

### W6. Handle inheritance is opt-in and listed

- **Source:** <https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw>
- **Publisher:** Microsoft Learn
- **Dated:** current Win32 API page; accessed 2026-08-19
- **Quote:** "If this parameter is TRUE, each inheritable handle in the calling process is inherited by the new process. If the parameter is FALSE, the handles are not inherited. Note that inherited handles have the same value and access rights as the original handles." Also: "Applications can use the UpdateProcThreadAttributeList function with the PROC_THREAD_ATTRIBUTE_HANDLE_LIST parameter to provide a list of handles to be inherited by a particular process."
- **Use:** Strict spawn uses `bInheritHandles = FALSE` except for an explicit `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` allowlist (stdio / authenticated link only). A leftover host handle in the child is a failed proof.

---

## Linux

### L1. Namespace types isolate mount, PID, network, and user identity

- **Source:** <https://man7.org/linux/man-pages/man7/namespaces.7.html>
- **Publisher:** Linux man-pages project (Michael Kerrisk)
- **Dated:** man-pages 6.18, 2026-02-08
- **Quote:** "A namespace wraps a global system resource in an abstraction that makes it appear to the processes within the namespace that they have their own isolated instance of the global resource." Table: Mount/`CLONE_NEWNS`, PID/`CLONE_NEWPID`, Network/`CLONE_NEWNET`, User/`CLONE_NEWUSER`.
- **Use:** The Linux candidate requires user + mount + PID + network namespaces together. One namespace type is not the profile. `CLONE_NEWNS` still needs the host-path deny in L9; it is not itself a filesystem deny.

### L2. Creating most namespaces needs `CAP_SYS_ADMIN`; user namespaces are the exception

- **Source:** same as L1
- **Quote:** "Creation of new namespaces using clone(2) and unshare(2) in most cases requires the CAP_SYS_ADMIN capability … User namespaces are the exception: since Linux 3.8, no privilege is required to create a user namespace."
- **Use:** Unprivileged admission depends on creating a user namespace first, then the others. If user-namespace creation fails, the host does not start an unconfined child.

### L3. Namespace limits fail with `ENOSPC`

- **Source:** same as L1, `/proc/sys/user`
- **Quote:** "Upon encountering these limits, clone(2) and unshare(2) fail with the error ENOSPC."
- **Use:** `ENOSPC` / `EPERM` / `EUSERS` on required namespace creation is an admission failure, not a silent fallback.

### L4. User namespaces isolate UIDs, keys, and capabilities

- **Source:** <https://man7.org/linux/man-pages/man7/user_namespaces.7.html>
- **Publisher:** Linux man-pages project
- **Dated:** man-pages 6.18 (same colophon family as L1); accessed 2026-08-19
- **Quote:** "User namespaces isolate security-related identifiers and attributes, in particular, user IDs and group IDs … the root directory, keys (see keyrings(7)), and capabilities (see capabilities(7)). … the process has full privileges for operations inside the user namespace, but is unprivileged for operations outside the namespace." Also: "Since Linux 3.8, unprivileged processes can create user namespaces, and the other types of namespaces can be created with just the CAP_SYS_ADMIN capability in the caller's user namespace."
- **Use:** After entering the user namespace, drop every capability (see L6) so "root inside the namespace" is not ambient authority on the host. UID 0 inside the namespace is not a privilege grant on host files.

### L5. `no_new_privs` blocks execve privilege gain and is required for unprivileged seccomp filters

- **Source:** <https://www.kernel.org/doc/html/latest/userspace-api/no_new_privs.html>
- **Publisher:** The Linux Kernel documentation
- **Dated:** current kernel docs; accessed 2026-08-19
- **Quote:** "Once the bit is set, it is inherited across fork, clone, and execve and cannot be unset. With no_new_privs set, execve() promises not to grant the privilege to do anything that could not have been done without the execve call." Also: "Note that no_new_privs does not prevent privilege changes that do not involve execve(). An appropriately privileged task can still call setuid(2) and receive SCM_RIGHTS datagrams."
- **Companion:** <https://man7.org/linux/man-pages/man2/seccomp.2.html> — "In order to use the SECCOMP_SET_MODE_FILTER operation, either the calling thread must have the CAP_SYS_ADMIN capability in its user namespace, or the thread must already have the no_new_privs bit set. … Otherwise, the SECCOMP_SET_MODE_FILTER operation fails and returns EACCES."
- **Use:** Strict Linux admission sets `PR_SET_NO_NEW_PRIVS` before seccomp. `no_new_privs` alone is not a filesystem or network sandbox. SCM_RIGHTS remains a **runtime** hostile-test case (not an `admit()` primitive).

### L6. Capabilities are independently droppable; the bounding set survives exec

- **Source:** <https://man7.org/linux/man-pages/man7/capabilities.7.html>
- **Publisher:** Linux man-pages project
- **Dated:** man-pages 6.18 family; accessed 2026-08-19
- **Quote:** "Starting with Linux 2.2, Linux divides the privileges traditionally associated with superuser into distinct units, known as capabilities, which can be independently enabled and disabled." Also: "The capability bounding set is a security mechanism that can be used to limit the capabilities that can be gained during an execve(2)." `prctl(2) PR_CAPBSET_DROP` removes bounding-set bits.
- **Use:** Strict profile empties permitted/effective/inheritable/ambient sets and drops the bounding set after namespace setup. A leftover `CAP_SYS_ADMIN` / `CAP_NET_ADMIN` / `CAP_SYS_PTRACE` fails the proof.

### L7. Seccomp-BPF is inherited by children if `clone`/`fork` remain allowed

- **Source:** <https://man7.org/linux/man-pages/man2/seccomp.2.html>
- **Publisher:** Linux man-pages project
- **Dated:** man-pages 6.18 family; accessed 2026-08-19
- **Quote:** "If fork(2) or clone(2) is allowed by the filter, any child processes will be constrained to the same system call filters as the parent. If execve(2) is allowed, the existing filters will be preserved across a call to execve(2)."
- **Use:** The filter must deny (or immediately kill) spawn/ptrace/mount/socket families that the profile does not explicitly need. It MUST deny `clone`/`unshare` with `CLONE_NEWUSER`, `setns` into a user namespace, and `clone3`. Allowing those calls without a descendant policy fails the descendant hostile test.

### L8. Landlock is an additional unprivileged layer, not a replacement for namespaces

- **Source:** <https://docs.kernel.org/userspace-api/landlock.html>
- **Publisher:** The Linux Kernel documentation (Mickaël Salaün)
- **Dated:** June 2026 (page header)
- **Quote:** "The goal of Landlock is to enable restriction of ambient rights (e.g. global filesystem or network access) for a set of processes. Because Landlock is a stackable LSM, it makes it possible to create safe security sandboxes as new security layers in addition to the existing system-wide access-controls."
- **Use:** Landlock is preferred *in addition* to the namespace + capability + `no_new_privs` + seccomp stack **and** the mount-table host-path deny (L9). Landlock MAY only **stack**. It MUST NOT implement or substitute for item 2. Landlock or seccomp alone cannot admit the strict state. Missing Landlock on a kernel that lacks it is recorded; it does not by itself fail the candidate if the required namespace stack and mount-table deny are present.

### L9. `CLONE_NEWNS` copies the parent's mount list

- **Source:** <https://man7.org/linux/man-pages/man7/mount_namespaces.7.html>
- **Publisher:** Linux man-pages project (Michael Kerrisk)
- **Dated:** man-pages 6.18, 2026-02-08
- **Quote:** "A new mount namespace is created using either clone(2) or unshare(2) with the CLONE_NEWNS flag. When a new mount namespace is created, its mount list is initialized as follows: If the namespace is created using clone(2), the mount list of the child's namespace is a copy of the mount list in the parent process's mount namespace. If the namespace is created using unshare(2), the mount list of the new namespace is a copy of the mount list in the caller's previous mount namespace."
- **Use:** Linux `strict` requires `CLONE_NEWNS` **and** a mount-table host-path deny (bind-mount allowlist, cover/unmount, or `pivot_root`) with tests that role-private paths still work. A copied host mount table is not containment. Landlock (L8) may only stack; it is not the item-2 deny.

---

## Sources consulted and rejected as proof

| Lead | Why it is not a proof |
|---|---|
| Architecture 03 §4.2 progressive sandbox sketch | Destination prose; names deprecated `sandbox_init` and insufficient Windows/Linux primitives |
| Chromium LPAC / seatbelt write-ups | Practitioner synthesis, not the OS contract |
| Apple Developer JS-only Hardened Runtime page | Official locator kept (M8); Quote stays empty; forum/search paraphrases are not Quote; HR-alone does not admit `strict` |
| Apple Developer JS-only `startAccessingSecurityScopedResource` page | Official locator kept (M7 companion); Quote stays empty; same fail-closed as M8 |
| `docs/research/` / Codex notes | Nested private research; not staged from this Keld PR |
| Electron / VS Code sandbox flags | Product policy, not this spec |

## OS proof status (this pass)

| Platform | Candidate named? | Hostile probe run? | Status |
|---|---|---|---|
| macOS | yes (App Sandbox + inherit/XPC rules) | no | **unverified** |
| Windows | yes (zero-capability LPAC + ACL + handle list + job) | no | **unverified** |
| Linux | yes (userns+mnt+pid+net, host-path deny, no_new_privs, cap drop, seccomp; Landlock additional) | no | **unverified** |
