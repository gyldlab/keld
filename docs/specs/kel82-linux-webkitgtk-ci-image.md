# Spec: pinned Linux WebKitGTK CI environment

Status: draft
Linear: KEL-82 · Owner: GYLDLAB · Updated: 2026-08-19

## 1. Goal & non-goals

Run Keld's Linux Rust matrix, MSRV check, and Xvfb GUI smoke without acquiring the
WebKitGTK toolchain from a live apt mirror during every pull request. The chosen
environment must be reproducible, immutable, reviewable, and fail visibly if unavailable
or different from the reviewed package set.

Non-goals:

- Increasing an apt timeout, retrying a mirror, accepting an alternate unpinned source,
  or skipping a Linux gate.
- Changing Keld product code, permissions, IPC, package behavior, or performance claims.
- Publishing an image before a maintainer approves registry ownership, retention,
  provenance, and access policy.

## 2. Spec refs

- `AGENTS.md` § CI dependency routing and § No workarounds
- `docs/agents/workflow.md` § Review: CI is the arbiter, humans are the architects
- `.agents/testing.md` § CI tiers
- GitHub Actions official workflow/container documentation (accessed 2026-08-19)

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a Linux Rust matrix, MSRV, or GUI job, when it starts, then no step executes
   `apt-get update` or `apt-get install`.
2. Given the pinned environment digest and reviewed package manifest, when either differs
   from the checked-in lock, then the job fails before Keld compilation.
3. Given the pinned environment, when the Linux matrix runs, then it executes the same
   selected-package clippy/nextest/doc commands and produces their normal pass/fail result.
4. Given the pinned environment, when MSRV runs, then it executes the existing Rust
   version discovery and selected-package `cargo check` result unchanged.
5. Given the pinned environment, when the Xvfb GUI job runs, then it builds
   `keld-host`, starts Xvfb, and observes the titled window with `xdotool`.
6. Given an unavailable registry image, invalid digest, missing package verification, or
   wrong container runtime prerequisite, when CI starts, then the affected job fails
   visibly; it never runs a host apt fallback or reports success by skipping.
7. Given a rebuild request, when the environment image is produced, then its source,
   package-version manifest, SBOM, provenance and immutable digest are attached to the
   reviewed release record.

## 4. Design

### First-principles decomposition

| Atom | Logical component | Independent evidence | Correctness oracle |
|---|---|---|---|
| A1 | Keld source/test behavior | KEL-81 PR #36 passed router, security, docs, macOS, Windows and Linux GUI before apt stalls in two other Linux jobs. | Keld commands run in the pinned environment unchanged. |
| A2 | Fresh-runner package acquisition | MSRV job `95979085798` stalled in `apt-get update` after Azure mirror failure/fallback and was cancelled before Cargo. | No per-PR apt command exists in Linux jobs. |
| A3 | Linux build inputs | WebKitGTK, JavaScriptCoreGTK, GTK3, libsoup3, pkg-config and build-essential are compile prerequisites, not application runtime authority. | Package manifest checks exact installed versions and `pkg-config` availability. |
| A4 | CI execution substrate | GitHub job containers require a Linux runner and default `run` shell to `sh`; checkout/action/runtime prerequisites must be deliberate. | Router, checkout, Rust, Bun and shell contract all execute in the image. |
| A5 | Supply-chain identity | A mutable image tag or unaudited registry pull makes the build environment unreviewable. | Immutable digest, SBOM, provenance and verified manifest are required before Keld commands. |

### Synthesis and reuse decision

- **Reuse:** retain the existing GitHub Actions workflow, KEL-81 job-level router,
  package selection, Windows/macOS host jobs, and actual Linux Keld commands.
- **Named unmet requirement:** fresh `ubuntu-latest` lacks a stable, locally available
  WebKitGTK development toolchain. Repeating apt acquisition turns mirror availability
  into a required Keld correctness dependency.
- **Candidate boundary:** a Linux-only job container image, referenced by immutable
  digest, carries only the OS build prerequisites. Its Dockerfile/package lock/SBOM and
  provenance become the environment source of truth. The image must provide the runtime
  prerequisites required by GitHub actions or the job must arrange them explicitly;
  container `run` steps use an explicit shell where Bash syntax is required.
- **Failure rule:** missing image/digest/manifest is a red CI job. There is no apt,
  mirror, cache, tag, host-runner, or permissive fallback.
- **Performance:** no product performance claim. The measure is CI determinism and
  elimination of live apt acquisition; collect cold pull/build timing only after semantic
  equivalence is proven.

## 5. Boundaries

- Implement in: a reviewed Linux CI-environment directory, CI workflow Linux jobs,
  environment manifest/verification tool, SBOM/provenance workflow, and agent/testing
  documentation.
- Must not touch: macOS/Windows job execution, Keld product crates, Cargo dependency
  declarations, wire protocol, permissions, or release/update artifacts.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1: Human selects image registry ownership/visibility/retention and approves this
  supply-chain boundary.
- [ ] T2: Add a reproducible Linux environment recipe plus exact package manifest and
  local verification script; produce no publish action yet.
- [ ] T3: Build an attested image, generate SBOM/provenance, publish under approved
  registry authority, and pin its digest in an environment lock.
- [ ] T4: Move Linux Rust matrix/MSRV/GUI jobs to the pinned environment; remove apt
  steps and add fail-closed digest/package verification tests.
- [ ] T5: Prove real PR runs on all Linux lanes; document rebuild/rotation and emergency
  image-revocation procedure.

## 7. Test plan

- Static workflow test: Linux jobs name the immutable environment digest and contain no
  executable apt command or host fallback.
- Environment test: package-version manifest, `pkg-config`, Rust, Bun, Bash, Xvfb and
  xdotool checks run inside the image.
- Integration: KEL-81 router test plus selected-package clippy/nextest/doc, MSRV and
  KEL-28 Xvfb smoke execute in a GitHub-hosted Linux container job.
- Negative controls: corrupt digest/manifest, remove a required package, remove an image
  runtime prerequisite, or reintroduce apt; each must fail the corresponding test/job.

## 8. Review gates triggered

- unsafe: none
- public API: none
- permission model: none
- dependency addition: supply-chain / container base and OS packages require review
- wire protocol: none
- CI workflow / CODEOWNERS: human sign-off required
- registry publishing / provenance / retention: human maintainer sign-off required

## 9. Perf impact

No application performance impact. Establish baseline/after cold CI duration and network
dependency count after correctness passes; do not claim a speedup from containerization.

## 10. Open questions

1. Which approved registry owns the image, and is it public or private?
2. Which attestation/SBOM signer and retention policy are approved for Keld CI images?
3. Should GUI run in the same base image or a layered immutable GUI image with its own
   digest, given Xvfb/xdotool are GUI-only additions?
