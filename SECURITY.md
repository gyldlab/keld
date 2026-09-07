# Security policy

## Current support

Keld is pre-alpha. There is no supported stable release or binary distribution yet.
Maintainers investigate reports against current `main` on a best-effort basis;
there is no backport or commercial response-time commitment. Include the exact
commit and platform because behavior differs across the current OS implementations.

The [product-status ledger](docs/engineering/product-status.md) distinguishes
implemented, partial and target capabilities. The [security architecture](docs/architecture/03-security.md)
is the intended contract, not certification that every boundary is exercised.
Do not assume planned sandboxing, profile isolation, native services or updates
are available merely because an API or specification exists.

## Report privately

Use [GitHub's private vulnerability reporting form](https://github.com/gyldlab/keld/security/advisories/new).
Private vulnerability reporting is enabled for this repository. Reports are
visible to the maintainers handling the advisory; do not put an unfixed
vulnerability, credentials or exploit details in a public issue or discussion.
If the private form is unavailable, open a public issue titled
“Private security reporting unavailable” without vulnerability details so a
maintainer can restore the private channel.

A useful report includes:

- affected commit/version, OS/version/architecture, Bun revision and configuration;
- expected boundary and observed behavior, with a minimal isolated reproduction;
- impact, attacker prerequisites and affected data or authority;
- minimized logs or a proof of concept with secrets and unrelated data removed;
- a preferred private contact and whether you want attribution.

Relevant surfaces include permissions/default-deny, principal and session
identity, IPC validation, native resources, navigation/preload isolation,
process containment/revocation, install/update integrity and development
supply-chain execution. Report uncertain boundary defects too; a missing feature
or unsupported API alone is not a security vulnerability.

Test only systems and data you are authorized to use. Avoid accessing other users'
data or disrupting live services. This policy does not authorize testing third-party
systems or promise legal immunity or a bounty.

## Response and coordinated disclosure

Maintainers aim to acknowledge within three business days and provide an initial
assessment or progress update within seven business days. These are goals for a
volunteer project, not guaranteed deadlines. If no acknowledgement arrives,
follow up in the private report; use the availability-only public issue if the
channel itself is broken.

An uninvolved maintainer reproduces and assesses the report, identifies an owner,
and discusses a fix, mitigation and disclosure timing with the reporter. Status
updates explain delays and the next evidence needed. A fix must retain a
regression/negative control and pass the relevant platform and review gates;
a passing mock or another OS does not prove the affected boundary.

Once a fix or safe mitigation is available, maintainers publish a security
advisory describing affected revisions, impact, remediation and acknowledged
limitations, and request a CVE where appropriate. Credit is opt-in. Disclosure
may be accelerated if users face active exploitation; coordinate timing through
the private report rather than promising a universal embargo.

Routine bugs belong in [public issues](https://github.com/gyldlab/keld/issues).
Conduct reports follow [the Code of Conduct](CODE_OF_CONDUCT.md).
