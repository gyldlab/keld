# Spec: Disable Bun runtime installation for supervised roles

- Status: draft
- Tracker: GH-593
- Owner: 0monish (tracker)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

A KELD-launched Bun application role may load its admitted installed dependencies. A missing Electron alias must produce a typed diagnostic before foreign code executes. The process must never resolve that missing import through Bun's install cache. Project analysis, package installation and alias selection remain separately owned.

## 2. Spec refs

- FACT: [Bun 1.4.2 run options](https://github.com/oven-sh/bun/blob/744846f844374847c902b5e7fd59b4342a51ef99/docs/snippets/cli/run.mdx#L160-L162), retrieved 2026-11-06: “Disable auto install in the Bun runtime”.
- FACT: [Bun 1.4.2 argument parser](https://github.com/oven-sh/bun/blob/744846f844374847c902b5e7fd59b4342a51ef99/src/runtime/cli/Arguments.rs#L1179-L1192) selects disabled global cache for `--no-install`. Requalify the actual shipped Bun executable.
- FACT: `crates/keld-runtime/src/role.rs:134` builds staged launch arguments for the validated boot mode and admitted transport preload. `crates/keld-runtime/src/role.rs:874` prepares each attempt. The ordinary command and Linux strict branch consume arguments differently at lines 898 and 967.
- FACT: [Original issue observation](https://github.com/gyldlab/keld/issues/593#issuecomment-6026231344) records Bun 1.4.2 loading a missing Electron import from an install cache. It does not prove a cold-cache network request.
- FACT: [Corrected resolution witness](https://github.com/gyldlab/keld/issues/593#issuecomment-6542603794) found an already installed ancestor Electron package under no-install. This was a resolution-only experiment. [GH497 M15–M18](https://github.com/gyldlab/keld/pull/1172) keeps graph admission distinct.
- FACT: #502 owns the alias exports. #499 owns the pinned installed graph. #452 owns approved declared environment. `docs/architecture/06-runtime-and-tooling.md:1` describes the supervised Bun contract.

## 3. Acceptance criteria

### AC-1: Reject a missing alias in a warmed cache

- **Given:** Use a controlled offline cache containing a sentinel Electron package. The project has no reachable installed Electron. Prove guard-off warm-cache resolution first.
- **When:** Launch the production role with a static Electron import, the qualified Bun binary and its actual startup arguments.
- **Then:** Reject before app or foreign package evaluation. Surface a stable KELD-COMPAT error with the import and install-fix guidance. Record separate markers and the actual child exit or prelaunch rejection.
- **Negative control:**
  - **Single fault:** Remove the no-install option from only the qualified Bun launch.
  - **At-risk precondition:** The isolated Bun control without upstream graph refusal demonstrably evaluates the warmed sentinel.
  - **Expected RED:** The flag-specific control observes the sentinel. Retain the separate end-to-end check even if earlier admission masks this mutation.

### AC-2: Preserve refusal with a declared role key

- **Given:** Use #452's approved declaration for DRAWIO_DISABLE_UPDATE=true and verify the key reaches a legitimate control role.
- **When:** Launch the missing-alias fixture through the same declared-environment production route.
- **Then:** Record the typed failure, zero foreign evaluation, no new permission and same no-install policy as the ordinary role.
- **Negative control:**
  - **Single fault:** Omit no-install only when an approved key is present.
  - **At-risk precondition:** The warmed unguarded runtime fixture is permissive and the legitimate control saw the declared key.
  - **Expected RED:** The declared-key no-install test observes foreign evaluation, while ordinary-key coverage remains independently meaningful.

### AC-3: Apply policy on each creation and restart

- **Given:** A legitimate generation zero runs and releases its app-link. Prepare a fresh generation with a missing dependency and a warmed cache.
- **When:** Restart under supervisor control. Repeat for each Bun role constructor and supported launch mode, including Linux strict.
- **Then:** All qualified Bun launch routes carry the policy before run/entry. Each generation gets its own graph admission and fresh app-link. Runtime cache fallback never succeeds.
- **Negative control:**
  - **Single fault:** Skip the policy at the successor-only command boundary.
  - **At-risk precondition:** The first generation reached its ready marker; a supervisor event acknowledges successor preparation.
  - **Expected RED:** The successor's captured Bun option list or warmed-cache sentinel exposes the missed policy.

### AC-4: Preserve a legitimate installation and transport preload

- **Given:** An admitted local @keld/electron package and qualified staged transport preload both have independent markers.
- **When:** Launch and restart with the exact approved package graph and app-link.
- **Then:** The local marker and real transport handshake complete. No package install runs. A blanket refusal or suppressing the legitimate preload fails.
- **Negative control:**
  - **Single fault:** Reject every bare import indiscriminately.
  - **At-risk precondition:** The real installed alias passes its independently owned identity check.
  - **Expected RED:** The local positive marker or authenticated handshake is missing.

### AC-5: Keep cache refusal distinct from installed graph identity

- **Given:** Place a foreign Electron package in a reachable ancestor or nested node_modules and keep the runtime flag enabled.
- **When:** Exercise both the isolated no-install probe and actual graph admission route.
- **Then:** The probe establishes no provenance for already-installed files. The graph owner must reject substitution before evaluation. Attribute the rejection to its actual layer.
- **Negative control:**
  - **Single fault:** Accept any installed package named electron.
  - **At-risk precondition:** Give the graph checker its valid root shim and a reachable nested foreign package for the actual importer.
  - **Expected RED:** The graph qualification or foreign sentinel fails independently of cache controls.

## 4. Design

### D-1: Set policy in the trusted Bun launch recipe

Select the Bun command-line option `--no-install`, before the subcommand or entry. An application-controlled bunfig is not a policy owner. A cold cache, offline flag or missing network cannot substitute for the warm-cache control.

Build the option once at the typed Bun role boundary shared by direct and contained launches. Distinguish generic process supervisors and non-Bun test executables. Keep the existing preload admission and validated define. Do not duplicate the policy by environment, OS or role name.

Trace every production caller of RoleConfig and every special Host launch path before changing it. For Linux strict, pass the completed argument plan into the confined command builder. Verify the final child-visible arguments without logging credentials or the app-link secret. A helper-unit test cannot cover an omitted caller.

Reject conflicting app-controlled launch settings at their existing authority boundary. Preserve #452 reserved-name filtering and trusted BUN_OPTIONS handling. Capture admission before the child can evaluate an app, app-supplied preload or foreign code. Repeat for each fresh generation.

### D-2: Keep diagnostics attributable

Missing or unapproved reserved imports use the compatibility owner’s registered diagnostic. #499/#502 bind the actual importer and installed graph before evaluation. A raw Bun stderr string, any exit 1, or all missing imports normalized to an Electron error cannot implement that classification.

Classify an unaffected missing non-Electron import under its actual owner. Preserve candidates' causes, role, attempt and safe remediation without printing environment values. Do not introduce a second JS resolver, global-cache sweep, network firewall, new bootstrap environment variable or a second admission registry.

### D-3: Keep measured effects separate

The normal integrated test may reject before spawn. Test the actual Bun command construction with valid authority and the warmed foreign fixture independently, so an earlier graph check cannot hide omission of the option. Never disable production admission as a public option.

UNKNOWN: No cold-network or all-platform pass follows from the historical warm-cache observations. When asserting zero network effects, observe attempted connections through an isolated local registry and qualify its control separately.

## 5. Boundaries

Runtime owns spawn preparation, leases and per-generation arguments. Host owns admitted role and config. Compat owns diagnostic vocabulary, #452 owns declared keys, #499/#502 own installed identity and exports. No change to wire, grants, package manifests, real caches or user configuration in this draft.

## 6. Tasks

### T-1: Land red conformance and hermetic witnesses

- **Blocked by:** none
- Record the KELD-specific oracle with its pinned Bun evidence and distinct expectation. Add positive local-shim and verified guard-off warm-cache tests.

### T-2: Bind the trusted flag to every Bun role launch

- **Blocked by:** T-1
- Trace each constructor, complete the typed arguments before backend selection, and cover prepared direct and strict successors. Run AC-1 through AC-4.

### T-3: Qualify integrated failure and declared keys

- **Blocked by:** T-2, B-1
- Attach the registered diagnostic, graph admission, declared-key and installed positive results. Record AC-5 on the real role path and publish per-platform limits.

## 7. Test plan & evidence

Use a private test cache, private fixture registry, deterministic module markers and supervisor events. Never alter HOME or a user's cache. Compare separately observed child attempts, exit classification, files and registry requests. Assert the guard-off baseline reaches the sentinel on the exact runtime before rating the guarded result.

### E-1: Pinned source audit

- **Run:** Read Bun's versioned option parser and documentation, KELD per-attempt preparation, the #593 brief and corrected #499 resolution evidence.
- **n:** One pinned KELD revision, one pinned Bun release and two historical issue receipts.
- **Environment:** GitHub connector, 2026-11-06. Source inspection only.
- **Limits:** No Bun process, network experiment or native runtime validation ran. Implementation must qualify each shipped binary and launch mode.

## 8. Review gates triggered

Public API for typed diagnostic and spawn semantics. Permission-model review covers Host-owned startup inputs. No new unsafe, dependency or wire protocol in this proposal.

## 9. Perf impact

No measured change. Record added preparation work separately if graph verification changes. The single argument itself does not justify runtime performance claims.

## 10. Open questions, blockers & unknowns

### B-1: Qualify actual declared environment and graph owners

- **Status:** open
- **Owner:** Runtime, #452, #499 and #502 maintainers.
- **Impact:** Full integrated pre-evaluation and declared-key acceptance depend on the real launch and installed graph.
- **Check:** Show the ordinary and declared-key positive/negative pairs on the actual shipped Bun binary. Include each successor and actual strict/direct argument construction.
