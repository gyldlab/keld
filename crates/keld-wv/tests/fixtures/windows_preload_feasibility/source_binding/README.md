# W1 source-binding preparation fixtures

This directory is an experimental evidence slice for GH1184 / PR1307 r17,
head `20a798976d03e76056c5edf4c0d6fb5775c5659c`. It does not implement or approve
Route D, a production classifier, a Windows trust root, a grant writer, a bridge,
or application admission. Product acceptance criteria AC39(a), AC44 and AC15 are
**NOT RUN**: the proposed `keld migrate` implementation and authoritative source
census are not supplied by this slice.

## Exact inputs

The unmodified upstream source is
[jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e](https://github.com/jgraph/drawio-desktop/tree/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e),
version 31.7.0. The GPL-3.0-only license is retained in `vendor/drawio/LICENSE`.
`source_manifest.json` records SHA-256, Git blob IDs and byte lengths for the
preload, main source, package metadata, Windows x64 builder configuration and
license. `source_audit.py` verifies those bytes and the reviewed main registration
anchors without executing source or configuration.

| Exact file | Bytes | SHA-256 |
| --- | ---: | --- |
| `src/main/electron-preload.js` | 3,201 | `1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98` |
| `src/main/electron.js` | 119,176 | `55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d` |
| `package.json` | 1,977 | `c3612ea5534ab3a2be739b2ae40752d13cf497ee401af84e72a0c8c18ac7d077` |
| `electron-builder-win.json` | 2,143 | `b77ece2d3e390e14aceab666fc196ef788e1a3c266c40d244e7da4d549892749` |

The per-file main-source hash is **not** the proposed aggregate `mainDigest`.
`input_census_limits.json` pins nine observed relative main imports by Git blob,
the dependency lockfile, three packaging hooks and the drawio submodule commit
`24b76c2cbd55d88e354042e8d329a2e4708972bc`. Those additional pins are GitHub tree
metadata only; their bytes are not vendored or SHA-256-audited here. Transitive
dependencies, generated renderer assets and delivered package contents remain
outside this bounded snapshot. The Windows builder's broad input patterns do not
constitute the missing approved Keld census.

## Reviewed behavior and admission limit

The seven sites below are **manual source review**, not classifier output.
All line numbers refer to the exact preload above.

| Site | Lines | r17 rule | Actual source behavior |
| --- | --- | --- | --- |
| Initial color mode | 14–15 | c | Reads `process.argv`. |
| `mainResp` | 61–75 | b | Routes by request ID and forwards selected data, or message/error fields. |
| `fileChanged` | 77–85 | b | Filters by registered path and forwards current/previous fields. |
| `request` | 89–103 | a | Assigns request ID, branches on watchFile, deletes listener, sends one message. |
| `registerMsgListener` | 104–110 | b | Forwards one inbound payload; omits trailing values. |
| `sendMessage` | 111–114 | a | Uses a page-selected channel and forwards one payload; omits trailing values. |
| `listenOnce` | 115–121 | b | Forwards one inbound payload; omits trailing values. |

The manifest anchors 24 unique literal main registration channels at 26 source
sites (the `app-load-finished` channel has three sites). These are observations,
not grants. The only literal preload sink candidate is `el:rendererReq`.
Computed `sendMessage` channels are UNENUMERABLE. Known application channels
`el:app-load-finished` and `el:isModified-result` remain unadmitted under that
candidate set. Every enabled-grants list in this slice is empty.

The original main constructor at `electron.js:748` uses `new BrowserWindow(options)`.
The object is assembled and changed before construction. That form does not fit
the proposed inline object-literal site tie, and Q2 owner records remain missing.
The B4 Windows source authority, Q15 complete census/serialization, B10 public
interface output and B11 reviewed-channel carrier require exact owner decisions.

## What the executable fixtures prove

`fixture_census.py` hashes an explicit small authored fixture set. It does not
resolve imports, run a build, evaluate a config, discover application files, or
claim a production source identity. Its encoding is expressly experimental:
UTF-8 path, tab, lowercase hexadecimal file SHA-256, LF; entries sorted by UTF-8
path bytes. This is an algorithm proposal for test sensitivity, not an approved
production serialization.

`census_golden.json` fixes the complete canonical bytes and aggregate hash.
The golden was computed independently using .NET SHA256 and an explicitly
ordered literal path list, without calling the Python implementation. The tests
cover order, empty input, UTF-8 ordering, another declared preload, helper/main
changes, exclusion of permissions metadata and missing declared input.

The no-execution test reads authored hostile main/config text while intercepting
Python execution/process primitives. It also compares all input bytes before
and after and checks that marker files are absent. This is instrumented fixture
evidence. It does not prove the behavior of actual `keld migrate`, nor claim an
operating-system-wide process monitor.

`classifier_cases.json` stores 32 authored source shapes and hand-reviewed r17
expectations. The JavaScript tests exercise representative argument forwarding,
omission, inbound filtering and branch/deferred-call behavior. They do not
analyze arbitrary JavaScript or produce a classification. The tests also execute
only the digest-checked authentic preload in a Node VM with minimal in-memory
Electron/process/window stubs. Independent callback captures observe
`mainResp`, `fileChanged`, `registerMsgListener` and `listenOnce` behavior.
The VM is a test harness; it is not a security or native isolation boundary.
The original main source, package scripts and configs are never executed.

## Commands and recorded results

Run from this directory on Ramani with Python 3.14.1 and Node v25.2.1:

```powershell
python -B .\source_audit.py
python -B .\test_source_audit.py
node --test .\source_behavior.test.mjs
```

The source audit exited 0. Python passed 16/16 tests. Node passed 13/13 tests.
The following commands each run exactly one temporary fault and **must exit 1**
with the named assertion failure:

```powershell
python -B .\mutation_check.py omit-other-preload
python -B .\mutation_check.py omit-helper
python -B .\mutation_check.py ignore-preload-digest
python -B .\mutation_check.py ignore-main-digest
python -B .\mutation_check.py include-permissions-self-reference
python -B .\behavior_mutation.py omit-rest-only
python -B .\behavior_mutation.py mislabel-omitted-arguments
```

All seven controls failed their intended assertion on 2026-11-04 UTC. The rest
mutation changed only the F-B wrapper's argument forwarding; the expected
complete payload remained fixed, and the actual sink lost the second and third
values. The annotation mutation catches a wrong manual corpus expectation;
it is not a production classifier mutation.

Both healthy suites then passed again, exits 0. The original deliberate failure
output and recovery are preserved in
`evidence/20261104T234229Z-source-checks.txt`. PowerShell's stderr conversion
inserts RemoteException text between some Python output lines; the named
assertion, test count and child exit status are retained. Temporary test
directories are scoped beneath this directory and removed on completion.
No native windows, application profiles or GUI resources are created here.

Before source work, a remote directory creation attempt encountered a transient
device-offline response and dependent nested writes returned ENOENT. Directory
creation was retried after connection recovery and both required directories
were verified before writes continued. Those original diagnostics are retained
in the handoff's tool transcript; no failed content was used as evidence.

The required next step is independent review of this bounded evidence, followed
by exact owner decisions for the missing trust/source/public-interface contract
and implementation of the shared classifier/bridge in its owning subsystem.
Native order/source/epoch/grant/revocation evidence belongs to the separate
Windows harness and cannot be inferred from this directory's results.
