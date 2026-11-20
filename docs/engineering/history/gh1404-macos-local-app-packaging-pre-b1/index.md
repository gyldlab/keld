# Historical GH-1404 macOS packaging specification parts

**Status: archived, non-normative.** These four files are retained byte-for-byte
as `.md.txt` historical snapshots, for the pre-B-1 implementation record that was
merged from PR #1510 at `beff4c720efeb8a6e9aed06a932de2cf088b1eef`.
They have historical task and blocker names, including an outdated use of
`B-1` for native acceptance. They do not grant specification approval,
implementation authority, or an installed-app acceptance result.

The current, exact-content approved contract is
[`GH-1404 local macOS packaging`](../../../specs/gh1404-macos-local-app-packaging/index.md)
at PR #1407 commit `76f891c806d419f6772a1ad44beca54b48b45660`.
Its indexed `01`–`05` parts alone own current requirements, tests, runtime,
and delivery gates. In particular, its B-2 and B-3 input qualifications and
AC-11/AC-17 installed module-resolution evidence remain open.

| Archived original | Historical content |
| --- | --- |
| [requirements.md.txt](requirements.md.txt) | Source links and earlier A1–A16 summaries |
| [design.md.txt](design.md.txt) | Earlier bundle and runtime design |
| [implementation.md.txt](implementation.md.txt) | Earlier tasks and test checklist |
| [acceptance.md.txt](acceptance.md.txt) | Earlier performance and blocker inventory |

No current spec/index imports these files. Keep them for traceability; do
not use their earlier approval or acceptance wording as a release gate.
