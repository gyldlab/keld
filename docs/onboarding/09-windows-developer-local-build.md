# Windows developer-local signed application build (KEL-96)

This is a **developer-local** procedure for `keld build --target windows-x64`, not Microsoft Store distribution, general installer qualification, installed-host admission, or a publicly trusted release. A self-issued certificate is acceptable for a local test only when it is an existing, explicitly selected code-signing certificate whose chain the developer machine already trusts for Authenticode verification. The build **must not** create/import/export certificates or private keys, modify trust, or bypass KEL-135/KEL-53 admission.

## Required existing inputs

- Windows x64 development host, KELD CLI and project with `keld.build.ts` using `signing: 'developer-local'`.
- Existing certificate in the developer's own `Cert:\CurrentUser\My`, with an accessible private key, valid date, Code Signing EKU and Digital Signature key usage. Keep its 40-hex thumbprint.
- An independently authenticated SignTool executable and its previously recorded SHA-256 pin. Use the exact tool path, never automatic signer/certificate selection.
- Existing 32-byte raw update public key; approved Git/Cargo/Rustc source tools, compiler Bun/TypeScript tool paths and SHA-256 pins, an exact source commit, and unique private `outDir`. The update public key does not authorize or create a private signing key.
- Resolved `typescriptRoot` and `outDir` must use ordinary absolute local-disk paths such as `C:\Work\keld`. Project-relative paths are valid when they resolve to that form. Extended-length `\\?\C:\...`, UNC and device aliases are refused before compiler capture, source-session creation, or executable signing because the local compiler binds imports through fixed `file:///` URLs.

## Derive the publisher scope from the selected certificate

From PowerShell at the repository root, without administrator elevation:

```powershell
$thumbprint = '<YOUR-EXISTING-40-HEX-CODE-SIGNING-CERT-THUMBPRINT>'
& .\tools\windows\developer_signer_preflight.ps1 -Thumbprint $thumbprint -DiscoverPublisherScope
```

The output is explicitly `DISCOVERY_ONLY`, **not** approval to sign. Copy its 64-character `publisherScope` into the project's closed literal `developerSigner.publisherScope` in `keld.build.ts`. The same closed `developerSigner` requires `certificateThumbprint`, `releasePublicKey`, pinned `signTool`, `git`, `cargo`, `rustc` and `cargoHome` paths. Set the compiled app's real identity and project input configuration separately. Do not paste the example development machine's paths, certificate, hashes, or public key into another developer's project.

## Validate before spending a build/signing transaction

```powershell
& .\tools\windows\developer_signer_preflight.ps1 `
  -Thumbprint $thumbprint `
  -ExpectedPublisherScope '<DERIVED-PUBLISHER-SCOPE>' `
  -SignToolPath '<PINNED-SIGNTOOL-EXE-PATH>' `
  -ExpectedSignToolSha256 '<INDEPENDENTLY-RECORDED-SIGNTOOL-SHA256>' `
  -ReleasePublicKeyPath '<EXISTING-RAW-32-BYTE-UPDATE-PUBLIC-KEY-PATH>'
```

Only a `PREFLIGHT_PASS` is a successful **metadata** preflight. It checks certificate identity, validity, key availability indicator, EKU, key usage, publisher scope from the original certificate SPKI DER, pinned SignTool bytes, and raw update public-key length. The SPKI parser in `tools/windows/developer_signer_spki.ps1` runs in PowerShell/.NET in-process; production preflight does not resolve or launch Python, a launcher, or another helper executable. It does **not** prove that the private-key operation will succeed, make the self-issued certificate publicly trusted, or qualify general release. A wrong scope/hash or missing input fails closed. Do not weaken checks to make a misconfigured developer project succeed.

Run the native read-only regression harness with an independently pinned Python `.exe` for its reference implementation and the existing SignTool metadata:

```powershell
$pythonPath = '<ABSOLUTE-PYTHON-EXE-PATH>'
$pythonSha256 = '<INDEPENDENTLY-RECORDED-PYTHON-EXE-SHA256>'
& .\tools\windows\test_developer_signer_preflight.ps1 `
  -Thumbprint $thumbprint `
  -PythonPath $pythonPath `
  -ExpectedPythonSha256 $pythonSha256 `
  -SignToolPath '<PINNED-SIGNTOOL-EXE-PATH>' `
  -ExpectedSignToolSha256 '<INDEPENDENTLY-RECORDED-SIGNTOOL-SHA256>' `
  -ReleasePublicKeyPath '<EXISTING-RAW-32-BYTE-UPDATE-PUBLIC-KEY-PATH>'
```

Python is test-only here. The harness uses the separately pinned interpreter with `-I -S -B` as an independent SPKI reference oracle, checks the in-process parser against Crypt32's `X509_PUBLIC_KEY_INFO` encoding, proves `.pth` and `sitecustomize` hooks execute in an ambient positive control, and places a marker-writing `python.cmd` first on `PATH`. Discovery and valid preflight must not launch that shim or write either startup marker. The production preflight itself has no Python dependency.

## Build and run

From the **application project root** containing its configured `keld.build.ts`:

```powershell
keld build --target windows-x64
```

Use the new unique session/output produced by this command. Keep the signed application-named `.exe`, co-bundled KELD CLI/Host, Bun and compiled app assets **together in their companion folder**. This is not a self-contained single-file EXE. Launch the signed application-named EXE, verify a real WebView2 window, bundled Bun, authenticated IPC and clean shutdown. Preserve build/launch receipts and verify the exact signed image and expected certificate; do not reuse a spent signing nonce or overwrite an earlier artifact.

This guide does not substitute for independent code review, another developer's certificate qualification, interactive sign-in testing, installed KEL-53 admission, Strict mode or public-release provenance. Microsoft Store, notarization and a publicly trusted CA are **not** prerequisites for this local build.
