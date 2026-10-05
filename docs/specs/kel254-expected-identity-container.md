# Spec: ExpectedAppIdentity Windows host container and writer
Status: draft
Linear: KEL-19 (packaging work: container and writer) · related KEL-254 amendment A3 and KEL-96 T3 Part B · Owner: GYLDLAB · Updated: 2026-10-05
Approval: pending. This is an approval draft. Implementation MUST NOT start until an
owner-approval receipt (a Linear comment that binds the PR head and this file's exact
SHA-256) is recorded in this header.
Owner decisions: the two product questions of the first draft (the packaging-input host
form and the app-id dual carrier) were decided by the owner on 2026-10-05 and recorded
by Linear KEL-19 comment `eebf7987-6c56-4fd5-9ad3-4d7096d055a6`; §4 "Owner decisions
(2026-10-05)" restates them. They are not the exact-content approval above.

KEL-254 amendment A3 (`docs/specs/kel254-windows-installed-root-boot.md` §4 "Selection
shape", §5, §6 T2b–T3 and §8) assigns three items to "the KEL-19 packaging work": the
writer that embeds the canonical `ExpectedAppIdentity` payload exactly once in
`keld-host.exe` before the executable's final Authenticode signature, the container that
holds it, and the `keld-cli -> keld-pack` edge. This spec defines those three items and
the `keld-pack` container reader that A3's `ExpectedAppIdentity::from_signed_image`
delegates to. It does not change the A3 text, the landed payload codec
(`crates/keld-pack/src/expected_identity.rs`, T2b) or the KEL-135 signature carrier.

## 1. Goal & non-goals

The Keld host is prebuilt once per Keld release and target; app developers never compile
it (Architecture 01 §1 and §2 principle 5). An installed Windows app still needs its own app id,
update channel, target and update-signing public key inside the signed host, because
A3 anchors executable-located installed boot on that expectation. The goal is one
container, written after link and before signing by one `keld-pack` writer, that the
Authenticode image hash covers, that occurs exactly once, and that `keld-update` reads
with bounded positioned reads from the same handle KEL-135 verified. The observable
outcome: a signed host whose container byte is altered fails `WinVerifyTrust`, and every
missing, duplicate, malformed or out-of-bounds container refuses with a typed error
before any boot resource exists.

Non-goals:

- no change to the payload codec, its domain tag or field bounds (T2b, landed);
- no change to the KEL-135 signed app-id carrier, publisher scope or profile identity;
- no Authenticode signing implementation, certificate or key custody, timestamp policy
  or signing-tool selection; those stay with the `keld build` signing step (Architecture
  06 §3);
- no installer, update-feed, `produce_windows_v0` or KEL-53 record change;
- no macOS or Linux container: neither platform has an installed-root successor (A3 AC2);
- no fallback carrier: a host without the container is refused, never read from a
  sidecar, resource, environment value or path;
- no stripping, re-signing or repair of an already signed image, and no Keld-signed
  packaging-input host (owner decision 1);
- no definition of the Keld release channel or of how it authenticates the
  packaging-input digest; this spec requires only that `keld build` takes the digest from
  that authenticated channel (T3 prerequisite, unowned: §10 item 1);
- no production `unsafe`, loader API, resource-update API or third-party dependency;
- no claim that the container grants authority: A3 admission remains the conjunction of
  the expectation, the located roots' file identities, the recorded mode's protection
  profile and the KEL-135 identity.

## 2. Spec refs

- `docs/specs/kel254-windows-installed-root-boot.md` (A3, approved): §4 "Selection
  shape" (`ExpectedAppIdentity`, `from_signed_image`, the KEL-19 ownership sentences),
  §5 (`keld-pack` owns the encoding, decoder and, with KEL-19, the container reader),
  §6 T2b–T3, §7 row "2 (T3)", §8 (public API, dependency and wire gates).
- `docs/architecture/01-overview.md` §1 (host prebuilt per platform), §2 principle 5
  (prebuilt host, no Rust toolchain for app developers; amended by this PR for the
  unsigned Windows packaging input as target behaviour), §3 (crate roles and
  dependency directions; `keld-pack` owns installer assembly and signing).
- `docs/architecture/06-runtime-and-tooling.md` §2 (`keld build` contract), §3
  (`keld-pack`, cross-host assembly target, signer tools), §4 client verification step 1
  (on Windows direct installs the prebuilt host carries the key, app id, channel and
  target in its signed `ExpectedAppIdentity` payload).
- `docs/specs/kel96-no-flag-host-boot.md` T1a (prebuilt `keld-host[.exe]` plus compiled
  app files) and `docs/specs/kel135-persistent-profile-identity.md` (Windows carrier:
  exactly one Authenticode signature; `SPC_SP_OPUS_INFO` program name
  `keld.app-id/v1:<canonical-app-id>`; no fallback carrier).
- `crates/keld-cli/src/verb.rs`: `keld build` is a reserved verb (`KELD-CLI-045`) whose
  tracking issue is KEL-19.

Deviation from architecture: one, corrected in the same PR. Architecture 01 §2
principle 5 read "Prebuilt signed host + npm distribution", which owner decision 1 makes
wrong for the Windows packaging input; this PR amends it to say that the host in a
shipped app is signed, and that the unsigned, digest-verified Windows packaging input is
target behaviour (KEL-19 T3, blocked on an authenticated Keld release channel). Its
onboarding copies and Architecture 07 §6 item 7 receive the same wording. No approval
digest or frozen block covers those lines. When T3 adds the
`keld-cli -> keld-pack` edge, the same PR updates the `keld-cli` "Depends on" cell of
Architecture 01 §3.

## 3. Acceptance criteria (binary, each becomes a test)

1. **Writer round-trip.** Given an admissible unsigned host image (a synthetic PE32+
   fixture on every CI OS, including one with an uninitialized-data section whose
   `SizeOfRawData` and `PointerToRawData` are both zero, and the workspace-built release
   `keld-host.exe` on Windows through the §7 mechanism) and a canonical payload, when
   the writer embeds it, then the container reader returns
   a payload equal to the input, the output length is the input length plus the
   container's `SizeOfRawData`, and the output differs from the input only in the byte
   ranges listed in §4 "Writer contract" (byte-diff oracle).
2. **Loadability.** Given the writer's output from the real unsigned release
   `keld-host.exe`, when it is launched on Windows with no dev lease, then the process
   starts and exits with the existing KEL-135 `KELD-WV-009` refusal before any listener,
   child or window. A loader rejection of the image fails this row.
3. **Exactly once.** Given the writer's own output, or any image that already has a
   section named `.keldeai`, when the writer runs, then it returns `KELD-PACK-008` and
   no output bytes.
4. **Embed before signing.** Given an input whose certificate data-directory entry is
   non-zero, or that has any byte after its raw end E (§4 writer step 1), when the
   writer runs, then it returns `KELD-PACK-009` and no output bytes.
5. **No room.** Given an input with 96 sections, fewer than 40 bytes between the section
   table end and either `SizeOfHeaders` or the lowest raw-data offset, a non-zero byte in
   those 40 bytes, or a non-zero bound-import directory, when the writer runs, then it returns
   `KELD-PACK-010` and no output bytes.
6. **Malformed input.** Given each independent header mutation listed in §7, when the
   writer or the reader runs, then it returns `KELD-PACK-006` (or the more specific code
   §4 assigns), never panics, and allocates no buffer proportional to the file size.
7. **Determinism.** Given the same input bytes and payload, when the writer runs twice
   and on Linux, macOS and Windows CI, then every output is byte-identical and equals one
   checked-in SHA-256 golden digest for the synthetic fixture.
8. **Coverage falsifier.** Given the writer's output signed once (one primary signature,
   no secondary) by the same operator-signed fixture path the KEL-135 Windows rows use,
   when `WinVerifyTrust` runs with the KEL-135 policy, then the intact image returns zero
   and the reader returns the embedded payload; flipping (XOR `0x01`, each on a fresh
   copy) one payload byte, one padding byte or the low byte of the container's
   `VirtualSize` in its section header each makes `WinVerifyTrust` return exactly
   `TRUST_E_BAD_DIGEST` (`0x80096010`, "The digital signature of the object did not
   verify."), and any other status fails the row; and, as the negative control,
   flipping one `CheckSum` byte still returns zero. The receipt records every exact
   status.
9. **Reader refusals.** Given an image with no `.keldeai` section, two such sections, or
   a single one that is not canonical (§4 "Reader contract"), when the reader runs, then
   it returns `KELD-PACK-007`, `KELD-PACK-008` or `KELD-PACK-011` respectively; given a
   canonical container whose payload bytes are not canonical, it returns
   `KELD-PACK-005`. Through `ExpectedAppIdentity::from_signed_image` these become
   `KELD-UPDATE-019` and `KELD-UPDATE-017` (A3 T3 proves they occur before listener,
   child or window).
10. **Handle-only reader.** Given an open `File` on Windows, when `read_host_identity`
    runs, then it uses only positioned reads on that handle and its handle-derived
    length; it opens no path and calls no loader or resource API. `keld-pack` contains no `unsafe` and no
    `LoadLibrary`, `FindResource` or `UpdateResource` reference.
11. **Build order.** Given the first `keld build` step that prepares a Windows host
    (T3), when it runs, then it verifies the packaging-input digest (AC12) before it
    embeds, embeds before the final signature, refuses a signed input with
    `KELD-PACK-009`, derives the payload app id and the KEL-135 program-name app id from
    one configuration value, and after the app publisher's signature checks that the
    reader returns the same payload, that the attribute certificate table starts at the
    container's raw end and ends at end of file, and that every byte before it equals
    the embedded output except the 4-byte `CheckSum` and the 8-byte Certificate Table
    entry. A failed post-sign byte check refuses with a second new typed `KELD-CLI`
    error (the next free number when T3 lands) and no package output.
12. **Packaging-input digest.** Given a packaging-input host whose SHA-256 differs from
    the digest that the authenticated Keld release channel publishes for that Keld
    version and target, or a digest that is missing or fails that channel's
    authentication, when `keld build` runs, then it refuses with a typed `KELD-CLI` error
    (the next free number when T3 lands) before embedding, and writes no host output.
13. **Signature carrier check.** Given a signed host on a Windows build host, when step 7
    of §4 runs, then it obtains the identity through KEL-135's existing verifier (the
    same `WinVerifyTrust` policy, exactly one primary and no secondary signature, and
    the `keld.app-id/v1:` program name) and refuses, with that verifier's
    `KELD-WV-009`, a host that fails it or whose decoded app id differs from the
    configured one. Given a non-Windows build host, or a publisher who signs outside
    `keld build` and does not return the signed host to step 7, then the build output
    states that the carrier check did not run, and a divergence is detected only at
    boot, where A3's equality chain fails closed (atom 10).

## 4. Design

### First-principles and reuse decision

| Atom / owner | Boundary and input → output | Failure mode | Independent observable |
|---|---|---|---|
| 1. Coverage / Microsoft Authenticode format; proven by T2 | signed PE image → the byte ranges the image hash covers | container bytes in an excluded or unhashed range, so a changed expectation still verifies | AC8: payload, padding and section-header flips fail `WinVerifyTrust`; the `CheckSum` flip control passes |
| 2. Container choice and placement / `keld-pack` | unsigned prebuilt host → exactly one new section after the existing sections | placement in overlay, the certificate table, headers or an ambiguous resource tree | AC1 byte-diff oracle; AC9 overlap and position refusals |
| 3. PE surgery validity / `keld-pack` writer | admissible host + payload → loadable image with existing bytes unmoved | loader refuses the image, or an existing section, directory or RVA shifts | AC1 byte-diff; AC2 real launch reaches the Keld typed refusal |
| 4. Signed-state ordering / writer + `keld build` | certificate directory and trailing bytes → unsigned admission only | embedding into a signed image silently breaks or strips a signature | AC4 refusal; AC8 sign-after-embed verifies |
| 5. Uniqueness / writer + reader | section table → exactly one container | missing or duplicate container resolved by choosing one | AC3; AC9 missing and duplicate rows |
| 6. Determinism / writer | bytes → bytes | host, time, locale or iteration-order dependence | AC7 golden digest on three OSes |
| 7. Reader bounds and trust / reader | handle + handle length → payload or typed refusal | an unchecked field causes an out-of-range read, panic, unbounded allocation, or reliance on an unhashed byte | AC6 and AC9 per-field mutations; fuzz target (§7) |
| 8. Handle binding / A3 T3 (KEL-96 consumer) with KEL-135 | the KEL-135-verified, write- and delete-share-denied handle → reader input | reading another object (a path reopen or the loader-mapped module) | reader signature takes `&File` only (AC10); A3 T3 rows; not owned here |
| 9. Payload semantics / `keld-pack` codec (unchanged) + `keld-update` | container bytes → `ExpectedAppIdentity` | non-canonical bytes, unsupported channel or invalid key admitted | existing T2b golden vector and refusals; AC9 `KELD-PACK-005` row |
| 10. App-id dual carrier / `keld build` single source + A3 equality chain (owner decision 2) | one configured app id → KEL-135 program name and payload app id, equal by construction, or refusal | the two carriers diverge silently | A3 equality chain at boot (record = expectation, record = KEL-135); AC11 single-source build; AC13 carrier check on Windows build hosts |
| 11. Build ordering / `keld-cli` (`keld build`, T3) | configuration + digest-verified prebuilt host → payload, embed, publisher signature, post-sign check, package | sign before embed, or a PE edit between embed and sign | AC11 |
| 12. Packaging-input host provenance / release channel: Owner: unassigned. No Linear issue currently owns the authenticated Keld release channel (digest publication and its authentication). First action: the repository owner assigns it to an existing issue (the owner prefers reusing existing, even closed, issues) or creates one. Verification: `keld build` (T3) (owner decision 1) | unsigned packaging-input host bytes + authenticated release digest → admitted input or refusal | a tampered or substituted input host is embedded and then signed by the app publisher | AC12 digest-mismatch, missing-digest and unauthenticated-digest refusals before embedding; the channel itself is a T3 prerequisite (§6) |

Independence edges. Atom 1 holds for any bytes inside a section's raw data, whatever the
writer does; atom 3 (loadability) is a separate property and is proven by a real launch,
not by coverage. The reader (atom 7) authenticates nothing by itself: its output is
authentic only through the explicit edge "atom 8 then atom 1", that is, the bytes were
hashed by `WinVerifyTrust` on the same handle and the handle's sharing prevented writes
between verification and read. Atom 10 does not depend on the container: A3 already
requires the KEL-53 record to equal both the expectation and the KEL-135 identity, so a
divergent pair refuses at boot even if the build check (AC11) were missing. Atom 12 is
upstream of every other atom and no other atom closes it: the publisher's signature
authenticates whatever host was embedded, so only the AC12 digest check binds that input
to the Keld release.

Security decomposition. *Identity:* `keld build` mints the expectation from the
developer's configuration; KEL-135 mints publisher scope and app id from the signature;
no new principal exists. *Authentication:* before embedding, the unsigned
packaging-input host by its SHA-256 against the authenticated Keld release digest (atom
12); after signing, the single primary Authenticode signature, whose image hash covers
the container (atom 1). *Authorization:* none; the expectation
is compared, never obeyed (A3 conjunction). *OS containment:* not applicable at build
time; at boot, A3 T3 opens the image without write or delete sharing. *Lifecycle and
revocation:* certificate revocation follows the KEL-135 `WinVerifyTrust` policy
(whole-chain revocation); a changed key, channel or app id requires a new build and
signature. *Evidence provenance:* T2 binds the exact head, fixture digest, signer and
recorded statuses.

**Reuse.** `keld-pack` already owns the payload encoding and is reached by
`keld-update` (Architecture 01 §3); the container reader and writer join it and reuse
`ExpectedAppIdentityPayload` for every byte of content, so no second payload parser or
writer exists. The reader reuses KEL-135's verified handle instead of opening anything.
The fuzz target reuses the existing `crates/keld-update/fuzz` crate and its pinned
`libfuzzer-sys`. No platform loader, resource-update or image-help API is reused,
because each either takes a path, reads a different object or needs production
`unsafe` (rejected alternatives below). Compatibility fallback: not required; no
installed Windows boot exists yet (A3 T3 Part B is unlanded), so no shipped host lacks
the container.

### Owner decisions (2026-10-05)

The owner decided both product questions of the first draft on 2026-10-05, recorded by
Linear KEL-19 comment `eebf7987-6c56-4fd5-9ad3-4d7096d055a6`.

1. **Packaging-input host: unsigned, digest-verified.** Keld ships the prebuilt
   `keld-host.exe` packaging input unsigned. `keld build` verifies its SHA-256 against
   the digest published by an authenticated Keld release channel, then embeds the
   payload, and the app publisher then applies the final Authenticode signature. Rejected
   alternatives: (a) a Keld-signed packaging input plus a strip step that must reproduce
   the exact pre-signing bytes, which adds PE surgery and a second signature path; (b)
   shipping both an unsigned packaging input and a Keld-signed host. Consequences: the
   writer stays strip-free and refuses any signed input (AC4); `keld build` gains the
   AC12 digest check; Architecture 01 §2 principle 5 is amended in this PR (§2).
2. **App id carried twice: keep both carriers.** The KEL-135 signed description
   (`keld.app-id/v1:` plus the app id) and the `ExpectedAppIdentity` payload both stay.
   `keld build` writes both from one configured app id (AC11), and A3's equality chain
   (record = expectation, record = KEL-135 identity) refuses a mismatch at boot. Rejected
   alternative: amending A3 and KEL-135 so that one carrier holds every field (option E
   below). Neither approved spec is reopened.

### Authenticode coverage (atom 1 evidence)

Two Microsoft primary sources describe the image hash. Both were retrieved on
2026-10-05; the receipt in Appendix A records versions and digests.

The [PE Format page](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)
(`ms.date` 2026-09-10), Appendix A "Calculating Authenticode PE Image Hash":

- Sections and resources: "All data in sections of the PE image that are specified in
  the section table are hashed in their entirety except for the following exclusion
  ranges:". Resources are section data: the page documents the resource tree
  ("Resources are indexed by a multiple-level binary-sorted tree structure") as the
  content of "The .rsrc Section".
- Checksum excluded: "The file CheckSum field of the Windows-specific fields of the
  optional header. This checksum includes the entire file (including any attribute
  certificates in the file)."
- Certificate directory and table excluded: "Authenticode excludes the following
  information from the hash calculation:" "The Certificate Table field of the optional
  header data directories." and "The Certificate Table and corresponding certificates
  that are pointed to by the Certificate Table field listed immediately above." The
  Certificate Table entry is data-directory index 4, at optional-header offset 128
  (PE32) or 144 (PE32+), and "The virtual address value from the Certificate Table entry
  in the Optional Header Data Directory is a file offset to the first attribute
  certificate entry."
- Data after the last section: "Information past of the end of the last section. The
  area past the last section (defined by highest offset) is not hashed." (sic)
- Placement: "attribute certificate and debug information must be placed at the very
  end of an image file, with the attribute certificate table immediately preceding the
  debug section, because the loader does not map these into memory."

The [Windows Authenticode Portable Executable Signature Format](https://download.microsoft.com/download/9/c/5/9c5b2167-8017-4bae-9fde-d599bac8184a/Authenticode_PE.docx)
(Version 1.0, March 21, 2008), "Calculating the PE Image Hash":

- Header: step 3 "Hash the image header from its base to immediately before the start
  of the checksum address", step 4 "Skip over the checksum, which is a 4-byte field.",
  step 7 "Exclude the Certificate Table entry from the calculation and hash everything
  from the end of the Certificate Table entry to the end of image header, including
  Section Table (headers)." and step 8 sets `SUM_OF_BYTES_HASHED` to `SizeOfHeaders`.
- Sections: step 9 "Do not include any section headers in the table whose
  SizeOfRawData field is zero." and step 11 "hash the entire section. Use the
  SizeOfRawData field in the SectionHeader structure to determine the amount of data to
  hash."
- Data after the sections: step 14 "If FILE_SIZE is greater than SUM_OF_BYTES_HASHED,
  the file contains extra data that must be added to the hash. This data begins at the
  SUM_OF_BYTES_HASHED file offset, and its length is: (File Size) – ((Size of
  AttributeCertificateTable) + SUM_OF_BYTES_HASHED)".
- Its own caveat: the procedure "is a simplified version of the procedure performed by
  ImageGetDigestStream and calculates the correct hash value for almost all
  Authenticode-signed PE files", and the document itself points to the PE Format page
  "For the latest information".

Result. Both sources state that the raw data of every section is hashed (the 2008
document skips sections whose `SizeOfRawData` is zero) and that the `CheckSum`, the
8-byte Certificate Table entry and the attribute certificate table are excluded. Header
coverage is stated by only one of them. The PE Format page never states it: it names
only those exclusions, and adds that "Based on the parameters that are passed to
ImageGetDigestStream, other data from the PE image can be omitted from the hash
computation". The PE page therefore implies header coverage (excluding two header
fields means nothing unless the rest of the header is hashed), the 2008 document states
it (step 7, through `SizeOfHeaders` and including the section table) while calling its
own procedure simplified, and AC8 is the direct proof, because its `VirtualSize` flip
lies in the container's section header. **They contradict each
other on data after the last section (overlay):** the PE Format page says it is not
hashed; the Authenticode document's step 14 hashes it. This spec therefore places
nothing in the overlay and relies only on facts that neither source contradicts and
that AC8 tests directly. AC8 is the falsifier for
the chosen placement; it does not try to settle the overlay question, which no Keld
decision depends on.

### Container choice

| Option | Hash coverage | Unique and bounded | Read from the verified handle without a loader or `unsafe` | Writable on every `keld build` host | PE surgery risk |
|---|---|---|---|---|---|
| A. Fixed-name `RT_RCDATA` resource | yes (section data) | needs type/name/language rules; one name may hold several languages | no: `FindResourceW` with a null module "searches the module used to create the current process" (a different object) and `LoadLibraryExW` takes a file name; both are FFI. A handle parser must walk a three-level tree and translate RVAs | `BeginUpdateResourceW` is Windows-only and path-based; elsewhere a full resource-tree serializer is needed | highest: the observed Rust/MSVC executable (Appendix A) has no `.rsrc`, so a section **and** a tree are created; growing an existing `.rsrc` moves later sections |
| **B. Dedicated new section `.keldeai` (chosen)** | yes (section data, both sources) | exact 8-byte name; exactly one; payload length bounded by the codec | yes: section-table walk plus one bounded positioned read | yes: pure byte transformation | low: one 40-byte header in existing zero slack, four header fields, an appended raw block; any other layout refuses |
| C. Overlay after the last section | contradictory between the two sources | no structure | yes | yes | low, but coverage is not established |
| D. Link-time reserved slot in `keld-host` patched in place | yes | yes | yes | yes | lowest for the writer, but needs `#[unsafe(link_section)]` plus a `#[used]` static in `keld-host` |
| E. Extend the signed `SPC_SP_OPUS_INFO` program name | signed attribute, outside the image | KEL-135 grammar allows the app id only | no: needs the KEL-135 WinTrust FFI | signer-dependent | changes two approved contracts |
| F. Sidecar file beside the executable | no | n/a | n/a | yes | n/a |

Recommendation: **B**, a dedicated new section appended by the `keld-pack` writer after
link and before signing.

Rejected alternatives and why:

- **A** fails the handle criterion: the in-process `FindResourceW` reads the loader-mapped
  module rather than the KEL-135-verified handle, `LoadLibraryExW` would reopen by path
  (A3: `keld-update` "never reopens the executable by path"), and both need FFI outside
  every sanctioned `unsafe` owner. Writing it cross-host needs a resource-tree
  serializer, and on the observed Rust/MSVC layout (Appendix A) it is strictly more
  surgery than B.
- **C** fails coverage: the two Microsoft sources disagree, and the PE Format page also
  requires the attribute certificate table at the very end of the image.
- **D** is the strongest runner-up. The rustc book states "The unsafe_code lint catches
  usage of unsafe code and other potentially unsound constructs like no_mangle,
  export_name, and link_section", and the Rust Reference states the `link_section`
  attribute "is unsafe as it allows users to place data and code into sections of memory
  not expecting them". The workspace sets `unsafe_code = "deny"` and
  `crates/keld-host/AGENTS.md` says "Production unsafe belongs to the owning library,
  not this binary." D would also depend on unproven linker behavior (the section is
  retained, unmerged and sized as declared). If the owner prefers zero post-link
  surgery, D needs an owner-instruction amendment and the `unsafe` gate first; this
  spec does not take that path.
- **E** would make KEL-135's approved carrier grammar (exactly the app id) carry updater
  policy, requires a second, text encoding of the payload, and contradicts A3's
  approved shape (embed before signing; read through `keld-pack`). Owner decision 2
  rejects it.
- **F** is not covered: A3's non-goals already exclude any claim that Authenticode on
  `keld-host.exe` authenticates neighboring files, and KEL-135 forbids sidecar carriers.

### Container format v1

The section header and raw data below form a persisted binary format in every signed
Windows host and are reviewed under the wire gate (§8).

| Field | Value |
|---|---|
| Name | the exact 8 bytes `.keldeai` (`2E 6B 65 6C 64 65 61 69`); case-sensitive; no terminating NUL, which the PE Format page permits: "If the string is exactly 8 characters long, there is no terminating null." |
| `VirtualSize` | the exact payload length L; L lies between the codec's minimum (68) and maximum (400), both derived from `keld-pack` constants, never mirrored |
| `VirtualAddress` | the input's `SizeOfImage` |
| `SizeOfRawData` | L rounded up to `FileAlignment` |
| `PointerToRawData` | the input's file length, which equals the raw end E defined in writer step 1 |
| `PointerToRelocations`, `PointerToLinenumbers`, `NumberOfRelocations`, `NumberOfLinenumbers` | zero |
| `Characteristics` | `0x40000040`: `IMAGE_SCN_CNT_INITIALIZED_DATA` (`0x00000040`) plus `IMAGE_SCN_MEM_READ` (`0x40000000`) |
| Raw data | the L payload bytes, then zero bytes up to `SizeOfRawData` |
| Position | the last header in the section table, the highest raw-data offset and the highest virtual address |

The payload's own domain tag (`keld.expected-app-identity/v1`) versions the content; a
future incompatible container uses a new section name, so a v1 reader refuses it as
missing rather than misreading it.

### Writer contract

Location: a new module `crates/keld-pack/src/host_identity.rs`, compiled on every
target, std only, no `unsafe`, no new dependency. The section name stays crate-private,
like the payload domain tag, so no second writer or reader exists outside `keld-pack`.

```rust
/// Embeds `payload` exactly once into an unsigned, unmodified prebuilt Windows x64
/// host image and returns the new image bytes. Pure and deterministic.
pub fn embed_host_identity(
    host: &[u8],
    payload: &ExpectedAppIdentityPayload,
) -> Result<Vec<u8>, PackError>;
```

The writer accepts only a validated `ExpectedAppIdentityPayload`, so it cannot embed
non-canonical bytes. It admits the input in this order and refuses at the first failure,
before allocating the output:

1. *Structure* (`KELD-PACK-006`): `MZ` signature; `e_lfanew` at offset `0x3C` is at
   least 64 and its `PE\0\0` signature and COFF header lie inside the input; `Machine`
   is `IMAGE_FILE_MACHINE_AMD64`; `Characteristics` has `IMAGE_FILE_EXECUTABLE_IMAGE`
   and not `IMAGE_FILE_DLL`; `SizeOfOptionalHeader` is 240, `Magic` is `0x20B` and
   `NumberOfRvaAndSizes` is 16; `FileAlignment` is a power of two from 512 to 65536
   ("The value should be a power of 2 between 512 and 64 K, inclusive"); `SectionAlignment`
   is a power of two of at least 4096 and at least `FileAlignment` (Keld policy that
   excludes the low-alignment layout in which "the physical offset for section data is
   the same as the RVA"); `SizeOfHeaders` is a multiple of `FileAlignment` and inside
   the input; the section table ends at or before `SizeOfHeaders`. A section whose
   `SizeOfRawData` and `PointerToRawData` are both zero holds only uninitialized data
   (the PE Format page says of each field: "When a section contains only uninitialized
   data, this field should be zero."); it has no raw range, is exempt from the raw-range
   rules below, and is skipped by the image hash (2008 document step 9). A section with
   exactly one of those two fields zero is refused. For every other (non-empty) section,
   `PointerToRawData` and `SizeOfRawData` are multiples of `FileAlignment` ("For
   executable images, this must be a multiple of FileAlignment"), and every raw range
   lies inside the input, at or after `SizeOfHeaders`, and the ranges do not overlap.
   The raw end E is the larger of `SizeOfHeaders` and the end of every non-empty
   section's raw range; empty sections never contribute to E or to any read. Virtual
   addresses are ascending and adjacent ("they must be a multiple of the
   SectionAlignment value") and `SizeOfImage` equals the last section's
   `VirtualAddress + VirtualSize` rounded up to `SectionAlignment` ("It must be a
   multiple of SectionAlignment"). All arithmetic is checked in `u64`, and every value
   written back must fit its `u32` field.
2. *Exactly once* (`KELD-PACK-008`): no section header is named `.keldeai`.
3. *Before signing* (`KELD-PACK-009`): both fields of the Certificate Table entry are
   zero, and the input length equals E, so there is no overlay, trailing debug data or
   attribute certificate table. The writer never
   strips or repairs a signature.
4. *Room* (`KELD-PACK-010`): `NumberOfSections` is at most 95, because "the Windows
   loader limits the number of sections to 96"; the 40 bytes after the section table
   lie entirely at or before both `SizeOfHeaders` and the lowest non-empty section's
   `PointerToRawData`, and are all zero;
   the bound-import directory (index 11), which would live in that header area, is zero.
   The writer never grows `SizeOfHeaders`, because that would move every section's raw
   data.

Output: a copy of the input in which exactly these bytes change, and nothing else
(`TimeDateStamp`, debug directory, existing sections and data directories are
unchanged):

- `NumberOfSections` (COFF header offset 2, 2 bytes): plus one;
- `SizeOfInitializedData` (optional-header offset 8, 4 bytes): plus the container's
  `SizeOfRawData`, which keeps the field's documented meaning, "the sum of all such
  sections";
- `SizeOfImage` (optional-header offset 56, 4 bytes): the container's
  `VirtualAddress + L` rounded up to `SectionAlignment`;
- `CheckSum` (optional-header offset 64, 4 bytes): zero. The field is excluded from the
  image hash, and the PE Format page lists only "all drivers, any DLL loaded at boot
  time, and any DLL that is loaded into a critical Windows process" as validated at load
  time. The page does not specify the checksum algorithm (it is "incorporated into
  IMAGHELP.DLL"), so the writer does not recompute it. A signer may set it later.
- the 40-byte container section header at the old section-table end;
- the appended raw data at the old file end.

Before returning, the writer reads its own output with the reader below and requires the
returned payload to equal its input; a mismatch is a writer defect reported as
`KELD-PACK-011` with no output. The function is a pure function of its two inputs: no
clock, randomness, environment, locale or unordered iteration.

### Reader contract (for A3 T3)

```rust
/// Reads the single expected-identity container from an open image using only
/// positioned reads on `image` and its handle-derived length.
#[cfg(windows)]
pub fn read_host_identity(image: &std::fs::File)
    -> Result<ExpectedAppIdentityPayload, PackError>;

/// The same parser over in-memory bytes: writer self-check, the `keld build` post-sign
/// check, the fuzz target and tests. Compiled on every target.
pub fn read_host_identity_bytes(image: &[u8])
    -> Result<ExpectedAppIdentityPayload, PackError>;
```

Both functions share one private parser over a crate-private positioned-read trait with
two implementations: byte slices on every target and `File` on Windows only.
`read_host_identity` is `cfg(windows)` because no non-Windows caller needs a handle read:
`from_signed_image` is Windows boot code (so it is `cfg(windows)` as well), and the
cross-host `keld build` check and the fuzz target use the bytes form. The `File`
implementation uses `std::os::windows::fs::FileExt::seek_read`, whose offset "is
relative to the start of the file and thus independent from the current cursor";
because "it is not an error to return with a short read", the reader loops until the
requested range is filled and treats end of file as a range beyond the file. The length
comes from the handle's metadata, never from a path. `seek_read` also moves the cursor
("The current cursor is affected by this function, it is set to the end of the read."),
and the reader neither depends on nor restores it. The KEL-135 handle is shared, so no
later user of it may rely on its cursor: A3 T3's later uses of that handle (the
file-identity comparison) MUST use cursor-independent calls.

Steps, each a typed refusal:

1. Admit the structure exactly as writer step 1 (`KELD-PACK-006`), reading at most the
   64-byte DOS header, the 264-byte PE signature, COFF and optional headers, and the
   section table (at most 96 × 40 bytes). Sections with both raw fields zero are exempt
   from the raw-range rules exactly as in writer step 1 and are never read. Because the
   section table must end at or before `SizeOfHeaders`, every header byte the reader
   interprets lies in the header range that the 2008 document hashes and AC8 tests for
   the container's header; the `CheckSum` is never read, and the Certificate Table entry
   is used only for the disjointness check in step 3.
2. Count section headers whose name equals `.keldeai` byte for byte: none is
   `KELD-PACK-007`; more than one is `KELD-PACK-008`.
3. Require the canonical form (`KELD-PACK-011`): it is the last header in the table;
   `Characteristics`, relocation and line-number fields equal the v1 values; L lies
   within the codec bounds; `SizeOfRawData` equals L rounded up to `FileAlignment`;
   `PointerToRawData` is aligned, at or after `SizeOfHeaders` and at or after the end of
   every other non-empty section's raw data; the raw range ends inside the file; its
   `VirtualAddress` equals the preceding sections' end rounded up to `SectionAlignment`
   and `SizeOfImage` equals its own end rounded up; and, when the Certificate Table
   entry is non-zero, the certificate table starts at or after the container's raw end
   and ends inside the file.
4. Read the L payload bytes and the `SizeOfRawData - L` padding bytes (fewer than
   65536) with positioned reads; non-zero padding is `KELD-PACK-011`.
5. Decode the L bytes with `ExpectedAppIdentityPayload::decode`; its refusal is
   `KELD-PACK-005`.

The reader allocates at most a few kilobytes plus the padding, independent of file size,
and never trusts a PE field before its bounds check. It authenticates nothing: A3 T3
calls it only on the handle that KEL-135 has just verified, so its result is authentic
by the edge "atom 8 then atom 1". `ExpectedAppIdentity::from_signed_image` (A3 T3, in
`keld-update`) calls `read_host_identity` and then applies the same channel and
Ed25519-key validation that `ExpectedAppIdentity::decode` applies, through one shared
internal path rather than a second check.

### Proposed typed errors

Proposed only; each is registered in `docs/engineering/keld-error-codes.md` in the PR
that first emits it, because that registry rejects headings no crate emits.

| Code | Variant (proposed) | Emitted by | Fix guidance |
|---|---|---|---|
| `KELD-PACK-006` | `HostImageInvalid` with a static detail | writer, reader | Use the unmodified prebuilt `keld-host.exe` of this Keld release; reinstall the signed package if an installed host is damaged. |
| `KELD-PACK-007` | `IdentityContainerMissing` | reader | Rebuild with `keld build` so `keld-pack` embeds the expected identity before signing. |
| `KELD-PACK-008` | `IdentityContainerDuplicate` | writer, reader | Embed exactly once into the unmodified prebuilt host; never re-run embedding on its output. |
| `KELD-PACK-009` | `HostImageNotPristine` with a static detail | writer | Embed into the unsigned prebuilt host before signing; never embed into a signed image or after appending data. |
| `KELD-PACK-010` | `HostImageNoRoom` with a static detail | writer | Use a Keld-released prebuilt host; a host without header room is a Keld host-build defect to report. |
| `KELD-PACK-011` | `IdentityContainerInvalid` with a static detail | reader, writer self-check | The host's identity container is damaged or was produced by another tool; reinstall the signed package or rebuild with `keld build`. |
| `KELD-PACK-012` | `IdentityContainerRead` carrying the `io::Error` as its source | `read_host_identity` (Windows) | Reading the host's identity container from the verified executable failed; make sure its volume is readable and relaunch, and reinstall the signed package if the failure persists. |
| `KELD-UPDATE-019` | `ExpectedIdentityContainer`, carrying the `keld-pack` code and detail | `from_signed_image` (A3 T3) | Reinstall the signed package or rebuild the host with `keld build`; installed boot refuses until the signed host carries exactly one valid container. |

Positioned-read I/O failures are `KELD-PACK-012`, not the package-production
`KELD-PACK-004`, whose fix ("Discard partial output … rebuild the package") is wrong for
a boot-time read of an installed host; they surface through `KELD-UPDATE-019`. Payload
refusals keep the landed `KELD-PACK-005` to `KELD-UPDATE-017` mapping.

### `keld build` order and the `keld-cli -> keld-pack` edge

Order for the Windows x64 direct cell:

1. **Link (Keld release, not per app).** Keld links `keld-host.exe` once per release
   and target and publishes it unsigned as the packaging input, together with its SHA-256
   on an authenticated Keld release channel (owner decision 1). App developers never
   compile it.
2. **Verify the packaging input.** `keld build` computes the SHA-256 of the unsigned
   host it was given and requires it to equal the digest that the authenticated channel
   publishes for that Keld version and target. A mismatch, a missing digest or a failed
   channel authentication refuses before any later step (AC12). The digest is never
   taken from the same unauthenticated location as the host bytes.
3. **App bundle and boot files.** Unchanged (the app's bundler and the KEL-96
   `keld.boot.json`).
4. **Payload.** `ExpectedAppIdentityPayload::new(app_id, channel, "windows-x64",
   update_public_key)`, where `app_id` is the single app-id configuration value also used
   for the KEL-135 description (owner decision 2) and `update_public_key` is the public
   half of the release's feed-signing key; no private key material is read into the
   payload.
5. **Embed.** `embed_host_identity` on the verified bytes of step 2. This is the last
   modification of the host before signing; any future PE edit (for example icons or
   version resources) must come before it, and none exists today.
6. **Publisher signature.** The app publisher applies the final and only Authenticode
   signature, with program name `keld.app-id/v1:` followed by the same app id and no
   secondary signature (KEL-135), through the `keld build` signer step or the
   publisher's own signing infrastructure. Keld never signs the packaging input.
7. **Post-sign check.** On the signed host, before packaging:
   `read_host_identity_bytes` returns the step 4 payload; the Certificate Table entry is
   non-zero; the certificate table starts at the container's raw end (already 8-byte
   aligned, because `FileAlignment` is at least 512) and ends at end of file; and every
   byte before it equals the step 5 output except the 4-byte `CheckSum` and the 8-byte
   Certificate Table entry, so the signer changed nothing else. On a Windows build host,
   step 7 also runs KEL-135's existing verifier on the signed host (AC13): exactly one
   primary and no secondary signature, and a `keld.app-id/v1:` program name whose app id
   equals the configured one. Step 7 runs only when `keld build` holds the signed host:
   after its own signer step, or when a publisher who signed elsewhere returns the
   signed host to it (T3 defines that invocation). Where the KEL-135 check cannot run
   (a non-Windows build host, or a signed host never returned to step 7), the build
   output says so, and a divergent program name is detected only at boot, where A3's
   equality chain fails closed (atom 10).
8. **Package.** Assemble the tree with the signed host, then `produce_windows_v0`, feed
   signing (KEL-53) and installer (KEL-53 and KEL-19).

Cross-host: steps 4 and 5 and the byte checks of step 7 are pure and host-independent,
and AC7 proves the writer's output identical on Linux, macOS and Windows. Step 2's
digest comparison is pure, but obtaining and authenticating the digest needs I/O
against the release channel, whose host support the channel's owner defines (§10).
Step 7's KEL-135 check runs only on Windows. Step 6 depends on the signer cell
(Architecture 06 §3 names signtool and an osslsigncode fallback); each signer qualifies
independently and is out of scope. Step 8 currently requires a Windows host
(`produce_windows_v0` returns `KELD-PACK-001` elsewhere). End-to-end Windows packaging
therefore still needs a Windows build host today; the writer adds no new host limit.

Edge: T3 adds `keld-cli -> keld-pack` together with the first real caller of step 5,
and not before. `keld-pack` never depends on `keld-cli`, `keld-core` or `keld-update`.
On Windows the edge brings `keld-pack`'s existing target-specific dependencies
(`keld-guard`, already a CLI dependency, and the workspace-pinned `blake3` and `zstd`)
into the CLI build; no new third-party crate enters `Cargo.lock`. A3 T3 Part B does not
need this edge: its tests build fixtures by calling the writer directly. Step 7's
carrier check reuses KEL-135 through the existing `keld-cli -> keld-core` edge: T3 adds
one thin public `keld-core` entry point over the existing private verifier
(`verified_windows_identity_from_image` and its trust-image open), with no second
Authenticode parser, no policy change and no new `unsafe`.

### Identity overlap with KEL-135

KEL-135 carries the app id in the signer's authenticated `SPC_SP_OPUS_INFO` attribute
(the Authenticode document: "The follow signed attribute is always present in an
Authenticode signature: SPC_SP_OPUS_INFO_OBJID (1.3.6.1.4.1.311.2.1.12)" (sic)), set at
signing time. The container sits inside the image hash and is written before signing.
Both are authenticated by the same single primary signature, and neither adds a
principal, so the container is not a second trust root. They overlap only in the app
id. Neither makes the other redundant: KEL-135's carrier holds the app id only and,
with the signer's publisher scope, feeds profile identity, while the container also holds the channel,
target and update key that A3 compares with the KEL-53 record. A3 already requires the
record to equal both, so divergence refuses at boot; AC11 removes it at build. The owner
decided to keep both carriers (owner decision 2).

### Template items

Capabilities required; manifest changes: none.

Wire/protocol changes: one new persisted binary format, the v1 container above, under
independent format review. The A3 payload, KEL-53 feed bytes, KEL-53 records and KIPC
frames are unchanged. A3 §4 lists the payload as its only wire change; the container
was delegated to this spec ("fixes its container") and is gated here.

Platform notes: Windows x64 only. The writer refuses any machine other than AMD64.
macOS and Linux have no container; the writer and `read_host_identity_bytes` still
compile and run their pure tests on every CI host, while `read_host_identity` and
`from_signed_image` are Windows-only.

Runtime seam: none at build time. At boot the seam is A3's: inside the host process,
before resources, `keld-core` passes the KEL-135-verified handle to
`ExpectedAppIdentity::from_signed_image`, which calls the `keld-pack` reader.

Migration unit: none. No installed Windows boot exists yet, so no shipped host lacks the
container; there is no persisted state, generated contract or adapter to migrate.

## 5. Boundaries

Implement in:

- `crates/keld-pack/src/host_identity.rs`, its tests, `crates/keld-pack/src/lib.rs`
  (module, re-exports, the seven `PackError` variants and their `Display`) (T1);
- `crates/keld-pack/Cargo.toml`: a `tempfile = { workspace = true }` dev-dependency
  for the Windows-only AC10 anonymous-file test (already workspace-pinned and in
  `Cargo.lock`) (T1);
- `crates/keld-pack/tests/real_host.rs`: an `#[ignore]` integration test that reads
  the release host path from `KELD_PACK_REAL_HOST` and fails, never skips, when the
  variable is absent (T1);
- `.github/workflows/ci.yml`: one Windows-only step in the `check` job, run when
  `keld-pack` or `keld-host` changes, that builds `cargo build --release -p keld-host`
  and runs `cargo test -p keld-pack --test real_host -- --ignored --exact` with
  `KELD_PACK_REAL_HOST` set to `target/release/keld-host.exe`, following the existing
  `--ignored --exact` pattern of that file. No `keld-pack -> keld-host` edge is added
  (T1);
- `crates/keld-update/fuzz` (one new `host_identity` target over
  `read_host_identity_bytes`, with a path dependency on `keld-pack`; no new pin) (T1);
- `docs/engineering/keld-error-codes.md`: `KELD-PACK-006` to `KELD-PACK-012` in T1,
  `KELD-UPDATE-019` in A3 T3 Part B, and the two new `KELD-CLI` codes (AC11 post-sign
  check, AC12 digest) in T3;
- the Windows signed-fixture acceptance under the existing KEL-135 operator path (T2);
- `crates/keld-cli/Cargo.toml` (the edge) and the `keld build` Windows host step,
  including the packaging-input digest check, in `crates/keld-cli`; one thin public
  `keld-core` entry point over the existing KEL-135 verifier for the AC13 check; and
  the Architecture 01 §3 `keld-cli` row (T3);
- `docs/architecture/01-overview.md` §2 principle 5, `docs/architecture/07-agent-experience.md`
  §6 item 7 and the matching `docs/onboarding/01-project-summary.md` and
  `docs/onboarding/02-architecture-guide.md` lines (this spec's PR; owner decision 1).

Must not touch:

- `crates/keld-pack/src/expected_identity.rs` encoding, domain tag and bounds;
- KEL-135 signature verification logic and policy, carrier grammar, publisher scope or
  profile hashing (T3 only exposes the existing verifier through one entry point);
- `crates/keld-host` source (no reserved slot, no `link_section`);
- KEL-53 records, feed schema, `produce_windows_v0` and installer code;
- `keld-guard` policy, generated permissions and KIPC;
- `docs/specs/kel254-windows-installed-root-boot.md`, whose A3 content is bound by its
  approval receipt.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T0 — exact-content owner approval of this spec, recorded in the header.
- [ ] T1 — in `keld-pack`: the writer, both reader entry points, the seven error
  variants and their registry entries, synthetic-fixture unit tests for AC1 and
  AC3–AC7, AC9 and the Windows-only AC10, the Windows real-host round-trip of AC1
  through the `real_host` test and its `ci.yml` step (§5), and the `host_identity` fuzz
  target with minimized regressions replayed as normal tests. This slice unblocks A3 T3
  Part B, which implements `ExpectedAppIdentity::from_signed_image` and
  `KELD-UPDATE-019` against it.
- [ ] T2 — on real Windows x64: AC2 (launch of the embedded unsigned release host) and
  AC8 (signed coverage falsifier with the `CheckSum` control), with a receipt that binds
  the source head, input and output digests, signer certificate, and every
  `WinVerifyTrust` status. A3 T3 Part B MUST NOT claim installed-boot acceptance before
  T2 passes.
- [ ] T3 — with the first `keld build` step that prepares a Windows host: the
  `keld-cli -> keld-pack` edge, the eight-step order of §4 (digest verification, embed,
  publisher signature, post-sign check), AC11 and AC12 with their two new `KELD-CLI`
  codes and registry entries, AC13 with its thin `keld-core` entry point, and the Architecture
  01 §3 row update. Prerequisite: a Keld release channel that publishes the unsigned
  packaging-input host and authenticates its SHA-256. None exists at base `7e1853f`
  (the only workflow building `keld-host` is CI, and no `@keld/cli` package exists), so
  T3 MUST NOT land until that channel and its authentication are specified and
  reviewed; T1, T2 and A3 T3 Part B do not depend on it. Owner: unassigned. No Linear
  issue currently owns the authenticated Keld release channel (digest publication and
  its authentication). First action: the repository owner assigns it to an existing
  issue (the owner prefers reusing existing, even closed, issues) or creates one.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1 | Synthetic PE32+ fixtures built in test code (no committed binary): one with two sections, one that adds an uninitialized-data section with both raw fields zero; embed, read back, compare payload; byte-diff the output against the input and fail on any change outside the six listed ranges. On Windows CI, the §5 `ci.yml` step builds the release `keld-host.exe` and runs the `#[ignore]` `real_host` test on it through `KELD_PACK_REAL_HOST`; the test fails, never skips, when the variable is absent. |
| 2 | Windows: launch the embedded unsigned release host lease-less; require exit with `KELD-WV-009` and zero listener, child and window attempts; a `CreateProcess` failure fails the row. |
| 3 | Embed twice; also embed into a fixture with a pre-existing `.keldeai` section; both `KELD-PACK-008`, no output. |
| 4 | Fixture with a non-zero Certificate Table entry; fixture with one trailing byte; fixture with trailing data and a zero entry; each `KELD-PACK-009`. |
| 5 | Fixtures with 96 sections, 39 bytes of slack, one non-zero slack byte, and a non-zero bound-import directory; each `KELD-PACK-010`. |
| 6 | One mutation per field, each independently: `MZ`, `e_lfanew` (small, beyond file, overflow), `PE\0\0`, `Machine`, DLL bit, `SizeOfOptionalHeader`, `Magic`, `NumberOfRvaAndSizes`, `FileAlignment` (not a power of two, 256, 131072), `SectionAlignment` (below 4096, below `FileAlignment`), `SizeOfHeaders` (misaligned, beyond file), section table beyond `SizeOfHeaders`, a section with only one of `SizeOfRawData` and `PointerToRawData` zero, misaligned or overlapping raw ranges, raw range beyond file, non-adjacent virtual addresses, wrong `SizeOfImage`, and `u32` overflow; each `KELD-PACK-006` from both writer and reader. The fuzz target asserts no panic and a fixed allocation ceiling. |
| 7 | Run the writer twice in one process and compare; the CI matrix on Linux, macOS and Windows compares the output SHA-256 with one checked-in golden digest. |
| 8 | Windows, real signature: intact image returns zero and decodes; XOR `0x01` into one payload byte, one padding byte, and the low byte of the container's `VirtualSize`, each from a fresh copy, each exactly `TRUST_E_BAD_DIGEST` (`0x80096010`); any other status, including another non-zero one, fails the row; XOR `0x01` into one `CheckSum` byte, zero. Record each exact status. |
| 9 | Missing, duplicate, and each non-canonical field of §4 reader step 3 (characteristics, L below 68 and above 400, `SizeOfRawData`, misaligned pointer, overlap with headers or another section, not last in table, file or address order, wrong `SizeOfImage`, overlap with the certificate table, non-zero padding); a canonical container holding a truncated payload gives `KELD-PACK-005`. |
| 10 | Windows: `read_host_identity` succeeds on an anonymous `tempfile::tempfile()` file (no path) after its cursor is moved to end of file, and an injected read failure gives `KELD-PACK-012`; on every OS, a source scan of `keld-pack` finds no `unsafe`, `LoadLibrary`, `FindResource` or `UpdateResource`. |
| 11 | `keld build` integration test (T3): signed input refuses with `KELD-PACK-009`; the step log shows digest verification, then embed, then signature; the post-sign check rejects with its `KELD-CLI` code, independently, an unsigned host, a fixture with bytes between the container and the certificate table and a fixture whose signer changed one byte before the certificate table outside the `CheckSum` and Certificate Table entry. |
| 12 | `keld build` integration test (T3): a packaging input with one flipped byte, a digest for another target or Keld version, a missing digest and a digest that fails channel authentication each refuse with the new `KELD-CLI` code before the writer runs; a writer-call counter stays zero and no host output exists. |
| 13 | Windows `keld build` test (T3) with operator-signed fixtures that pass the step 7 byte checks: a host signed with a different `keld.app-id/v1:` app id, a host with a secondary signature and a host signed by a certificate the build host does not trust each refuse in step 7 with `KELD-WV-009`; the matching single-signature host passes. On a non-Windows host, and on a Windows run whose signed host is never returned to step 7, the build output states that the carrier check did not run. |

Anti-flake: every writer and reader test is pure or uses one temporary file; no timing,
ports or sleeps. AC2 and AC8 are Windows-only real-OS rows and are not inferred from the
synthetic fixture or from CI on other hosts.

## 8. Review gates triggered

- unsafe: none. The writer and reader are safe Rust over byte slices and std positioned
  reads. Options A and D, which would need `unsafe`, are rejected.
- public API: yes — `embed_host_identity`, the Windows-only `read_host_identity`,
  `read_host_identity_bytes`, seven `PackError` variants, `KELD-UPDATE-019`'s variant
  (`from_signed_image` itself is already in A3's public-API list), and in T3 the thin
  `keld-core` entry point over KEL-135's verifier. Independent exact-diff API review.
- permission model: none — no capability, manifest, guard or grant change; the
  expectation confers no authority (A3 conjunction).
- dependency addition: yes — the internal workspace edge `keld-cli -> keld-pack` (T3);
  a path dependency of the existing `keld-update` fuzz crate on `keld-pack` (T1); and a
  `keld-pack` dev-dependency on the workspace-pinned `tempfile`, already in
  `Cargo.lock` (T1). No new third-party crate; the container module is std only. The
  T1 `ci.yml` step (§5) is a CI change reviewed under `.agents/ci.md`, not a dependency.
- wire protocol: yes — container format v1 (section name, characteristics, layout and
  canonical position) needs an independent format review. The A3 payload is unchanged.

## 9. Perf impact

None claimed. Installed boot adds a reader that performs a few positioned reads totaling
at most a few kilobytes plus the padding; it is measured inside A3's cold host-to-window
measurement rather than given its own budget. The writer is cold tooling that copies the
host once.

## 10. Open questions

1. **Owner of the authenticated Keld release channel (blocks T3 only).** Owner:
   unassigned. No Linear issue currently owns the authenticated Keld release channel
   (digest publication and its authentication). First action: the repository owner
   assigns it to an existing issue (the owner prefers reusing existing, even closed,
   issues) or creates one. Atom 12 is the only control that binds the unsigned
   packaging input to a Keld release, so T3 MUST NOT land until this owner exists and
   the channel is specified and reviewed. A Linear search on 2026-10-05 found only
   macOS-scoped neighbours: KEL-141 (prebuilt CLI and host distribution, Backlog) and
   KEL-103 (signed macOS prebuilt host spec). T1, T2 and A3 T3 Part B do not depend on
   it.

No product question remains: the packaging-input host form and the app-id dual carrier
were decided by the owner on 2026-10-05 (§4 "Owner decisions (2026-10-05)"), and format
and API review of the v1 container are review gates (§8).

## Appendix A. Current-documentation receipt

- Applicability: applied: Authenticode PE image-hash coverage; PE section, header and
  alignment rules; Windows resource and loader APIs; `WinVerifyTrust` result and
  `WINTRUST_FILE_INFO` handle semantics; Rust `unsafe_code`, `link_section` and
  `FileExt::seek_read` semantics.
- Context7: unavailable: the configured Context7 MCP server failed to connect at session
  start on 2026-10-05 (`CONNECTION_CLOSED`, "Connection closed"). Every claim below is
  confirmed from the primary source directly.
- Official primary, all retrieved 2026-10-05:
  - [PE Format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format),
    `ms.date` 2026-09-10 (page `updated_at` 2026-09-10T15:41:00Z); HTML SHA-256
    `6b8a2a4c36b85be9307ef36b6ca85635a7cbfe2ae808c6e5cc94fb9d0d5d7fd9`.
  - [Windows Authenticode Portable Executable Signature Format](https://download.microsoft.com/download/9/c/5/9c5b2167-8017-4bae-9fde-d599bac8184a/Authenticode_PE.docx),
    Version 1.0, March 21, 2008 (document history: updated URLs July 29, 2008; file
    properties modified 2020-08-10; HTTP `Last-Modified` 2025-02-17); file SHA-256
    `8f918de101954972b684cf83d552489b22b6ffaef81d5141e4320cebcb86fd85`.
  - [`LoadLibraryExW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw),
    `ms.date` 2024-07-15: `lpLibFileName` is "A string that specifies the file name of
    the module to load."
  - [`FindResourceW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-findresourcew),
    `ms.date` 2021-04-19: with a null `hModule`, "the function searches the module used
    to create the current process."
  - [`BeginUpdateResourceW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-beginupdateresourcew),
    `ms.date` 2018-12-05: `pFileName` is "The binary file in which to update resources.
    An application must be able to obtain write-access to this file; the file referenced
    by pFileName cannot be currently executing."
  - [Resource Types](https://learn.microsoft.com/en-us/windows/win32/menurc/resource-types),
    `ms.date` 2026-08-28: `RT_RCDATA` is `MAKEINTRESOURCE(10)`, "Application-defined
    resource (raw data)."
  - [`WINTRUST_FILE_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_file_info),
    `ms.date` 2018-12-05: `hFile` is "File handle to the open file to be verified."
  - [`WinVerifyTrust`](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust),
    `ms.date` 2018-12-05: "No other value besides zero should be considered a successful
    return."
  - [COM Error Codes (Security and Setup)](https://learn.microsoft.com/en-us/windows/win32/com/com-error-codes-4),
    `ms.date` 2018-05-31; HTML SHA-256
    `0ca536a0707b5c018235746dbce12cf7ad658fe775d9b6c6e936f9e41174aca8`:
    `TRUST_E_BAD_DIGEST` is `0x80096010`, "The digital signature of the object did not
    verify."
  - [rustc book, allowed-by-default lints](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html)
    (stable channel at retrieval): `unsafe_code` covers `no_mangle`, `export_name` and
    `link_section`.
  - [Rust Reference, `link_section`](https://doc.rust-lang.org/reference/abi.html)
    (stable channel at retrieval): the attribute is unsafe.
  - [`std::os::windows::fs::FileExt`](https://doc.rust-lang.org/std/os/windows/fs/trait.FileExt.html),
    std 1.99.0 documentation, `seek_read` stable since 1.15.0 (workspace MSRV 1.97):
    offsets are independent of the cursor, the cursor moves, and short reads are not
    errors.
- Supported claims: the coverage, exclusion, header-coverage and contradiction
  statements in §4 "Authenticode coverage"; the section-name, section-count, alignment,
  zero-raw-size, `SizeOfImage`, `SizeOfHeaders` and `CheckSum` rules in §4 "Container
  format v1" and "Writer contract"; the loader and resource-API facts in §4 "Container
  choice"; the AC8 `TRUST_E_BAD_DIGEST` value; the Rust facts behind rejecting option D
  and behind the reader's read loop and cursor rule.
- Local observation (not a platform claim): a workspace-built Rust 1.97.1 MSVC x64 test
  executable (`keld-update` test binary, SHA-256
  `08fb73604c08415d300b212e46d326b60a8ebc438e6fa9a7a799c46a96757cc2`), inspected
  read-only on 2026-10-05, has five sections ending in `.reloc`, no resource directory,
  `CheckSum` zero, `SizeOfHeaders` `0x400` with 296 zero bytes after the section table,
  `SectionAlignment` `0x1000`, `FileAlignment` `0x200`, and no bytes after the last
  section. This makes option B's admission plausible for the shipped host but proves
  nothing about it; T1 runs the admission on the workspace-built release
  `keld-host.exe`.
- Fallback/blocker: the overlay contradiction is left unresolved and is not decision
  bearing. Header coverage rests on the 2008 document and on AC8, not on the PE Format
  page. No other gap.
