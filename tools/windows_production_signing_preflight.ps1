# Read-only production signing identity *preflight*. No signing, key import, trust edits,
# timestamp request or release publication occurs here. Passing this check is not an
# approval to ship; independent signed publisher policy and artifact provenance
# remain required.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$PolicyPath,
    [Parameter(Mandatory=$true)][string]$ExpectedSourceCommit,
    [string]$SignToolPath,
    [switch]$SyntaxOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Windows PowerShell 5.1 can resolve user-planted scripts/modules before cmdlets.
# This is a non-elevated check, but it must not execute unqualified user PATH code.
$windowsRoot = [Environment]::GetEnvironmentVariable('SystemRoot')
if ([string]::IsNullOrWhiteSpace($windowsRoot)) { throw 'PROD-PRE-001: Missing Windows SystemRoot' }
$env:PATH = [IO.Path]::Combine($windowsRoot, 'System32') + ';' + [IO.Path]::Combine($windowsRoot, 'System32\WindowsPowerShell\v1.0')
$env:PSModulePath = [IO.Path]::Combine($windowsRoot, 'System32\WindowsPowerShell\v1.0\Modules')

function Refuse([string]$code, [string]$why) {
    throw ('PROD-PRE-' + $code + ': ' + $why)
}
function IsHex([string]$value, [int]$len, [bool]$upper) {
    if ($null -eq $value -or $value.Length -ne $len) { return $false }
    if ($upper) { return $value -cmatch '^[0-9A-F]+$' }
    return $value -cmatch '^[0-9a-f]+$'
}
function LocalAbsoluteFile([string]$path, [string]$what) {
    if ($path -cnotmatch '^[A-Za-z]:[\\/]' -or $path -match '(^|[\\/])\.\.?([\\/]|$)') {
        Refuse '002' ($what + ' must use an absolute local-disk path without dot traversal')
    }
    # Reject directory junction/symlink ancestors as well as a reparse file.
    # Testing just the leaf allows an otherwise local-looking path to redirect
    # through a reparse directory before policy/tool bytes are opened.
    $local = $path.Replace('/', '\')
    $parts = @($local.Substring(3).Split([char]'\'))
    $walk = $local.Substring(0, 3)
    foreach ($part in $parts) {
        if ([string]::IsNullOrEmpty($part)) {
            Refuse '002' ($what + ' has empty path segments')
        }
        # Win32 silently aliases trailing-dot and trailing-space names to a
        # different filesystem object. Reject aliases before opening policy or tool.
        # A non-drive colon also denotes an alternate data stream, not a file.
        if ($part -match '[. ]$' -or $part.Contains(':')) {
            Refuse '002' ($what + ' has a noncanonical Win32 path component')
        }
        $walk = [IO.Path]::Combine($walk, $part)
        if (-not ([IO.File]::Exists($walk) -or [IO.Directory]::Exists($walk))) {
            Refuse '003' ($what + ' path component not present')
        }
        $attrs = [IO.File]::GetAttributes($walk)
        if (($attrs -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Refuse '004' ($what + ' has a reparse-point path component')
        }
    }
    if (-not [IO.File]::Exists($path)) { Refuse '003' ($what + ' not present') }
}
if (-not (IsHex $ExpectedSourceCommit 40 $false)) { Refuse '005' 'Expected source SHA must be 40 lowercase hex' }
LocalAbsoluteFile $PolicyPath 'Policy'
$raw = [IO.File]::ReadAllBytes($PolicyPath)
if ($raw.Length -eq 0 -or $raw.Length -gt 16384) { Refuse '006' 'Policy byte size exceeds closed bounds' }
$text = [Text.UTF8Encoding]::new($false, $true)
try { $json = $text.GetString($raw) } catch { Refuse '007' 'Policy must be strict UTF-8' }
if ($json.Contains([char]92)) { Refuse '008' 'JSON escapes are not supported in the closed flat policy' }
# Flat, string-only policy makes exact key cardinality observable before the PS 5.1
# decoder can collapse duplicate JSON object keys. Unknown/duplicate keys fail.
$allowed = @(
    'schema', 'sourceCommit', 'publisherName', 'publisherCertificateThumbprint',
    'trustedRootThumbprint', 'rfc3161TimestampUrl', 'signerToolSha256',
    'updatePublicKeySha256'
)
$found = @([regex]::Matches($json, '"([A-Za-z][A-Za-z0-9]*)"\s*:'))
if ($found.Count -ne $allowed.Count) { Refuse '009' 'Policy has missing or duplicate JSON keys' }
$names = @($found | ForEach-Object { $_.Groups[1].Value })
foreach ($key in $allowed) {
    if (@($names | Where-Object { $_ -ceq $key }).Count -ne 1) { Refuse '010' ('Missing/duplicate policy key: ' + $key) }
}
try { $policy = $json | ConvertFrom-Json -ErrorAction Stop } catch { Refuse '011' 'Invalid flat JSON policy' }
if (@($policy.PSObject.Properties).Count -ne $allowed.Count) { Refuse '012' 'Unknown JSON policy fields' }
foreach ($property in $policy.PSObject.Properties) {
    if ($property.Name -cnotin $allowed -or $property.Value -isnot [string]) {
        Refuse '013' 'Unknown/non-string production policy field'
    }
}
if ($policy.schema -cne 'keld.windows.production-signing-policy/v1') { Refuse '014' 'Unsupported policy schema' }
if ($policy.sourceCommit -cne $ExpectedSourceCommit) { Refuse '015' 'Policy source differs from selected build SHA' }
if ($policy.publisherName.Length -lt 2 -or $policy.publisherName.Length -gt 120 -or $policy.publisherName -match '[\x00-\x1f<>]') {
    Refuse '016' 'Publisher name not bounded printable text'
}
foreach ($key in @('publisherCertificateThumbprint','trustedRootThumbprint')) {
    if (-not (IsHex $policy.$key 40 $true)) { Refuse '017' ($key + ' must be 40 uppercase hex') }
}
foreach ($key in @('signerToolSha256','updatePublicKeySha256')) {
    if (-not (IsHex $policy.$key 64 $false) -or $policy.$key -eq ('0'*64)) {
        Refuse '018' ($key + ' must be nonzero 64 lowercase hex')
    }
}
if ($policy.publisherCertificateThumbprint -eq $policy.trustedRootThumbprint) {
    Refuse '019' 'Publisher leaf and trusted root cannot be the same certificate'
}
$uri = $null
if (-not [Uri]::TryCreate($policy.rfc3161TimestampUrl, [UriKind]::Absolute, [ref]$uri) -or
    $uri.Scheme -cne 'https' -or
    $uri.Port -ne 443 -or
    $uri.HostNameType -ne [UriHostNameType]::Dns -or
    $uri.Host -notmatch '\.' -or
    $uri.UserInfo.Length -gt 0 -or
    $uri.Fragment.Length -gt 0 -or
    $uri.Query.Length -gt 0) {
    Refuse '020' 'RFC3161 endpoint must be explicit trusted DNS HTTPS origin without credentials/query/fragment'
}
if ($SyntaxOnly) {
    # Syntax validation alone MUST NOT authorize signing or publication.
    [pscustomobject]@{ scope='SYNTAX_ONLY_NOT_RELEASE'; schema=$policy.schema; sourceCommit=$policy.sourceCommit; productionReleaseQualified=$false } |
        ConvertTo-Json -Compress
    exit 0
}
if ([string]::IsNullOrWhiteSpace($SignToolPath)) { Refuse '021' 'Pinned SignTool path is required for native identity preflight' }
LocalAbsoluteFile $SignToolPath 'SignTool'
$toolSha = ([Security.Cryptography.SHA256]::Create()).ComputeHash([IO.File]::ReadAllBytes($SignToolPath))
$toolHash = [BitConverter]::ToString($toolSha).Replace('-','').ToLowerInvariant()
if ($toolHash -cne $policy.signerToolSha256) { Refuse '022' 'SignTool bytes differ from policy pin' }
$toolSignature = Get-AuthenticodeSignature -LiteralPath $SignToolPath
if ($toolSignature.Status -ne 'Valid') { Refuse '023' 'SignTool is not a trusted signed tool' }
$certs = @()
foreach ($store in @('Cert:\CurrentUser\My','Cert:\LocalMachine\My')) {
    $certs += @(Get-ChildItem $store -CodeSigningCert | Where-Object { $_.Thumbprint -ceq $policy.publisherCertificateThumbprint })
}
if ($certs.Count -eq 0) { Refuse '024' 'Policy publisher certificate absent from local code-signing stores' }
if ($certs.Count -gt 1) { Refuse '025' 'Ambiguous publisher certificate across local stores' }
$cert = $certs[0]
if (-not $cert.HasPrivateKey) { Refuse '026' 'Publisher signing key not available to selected service' }
if ($cert.Subject -ceq $cert.Issuer) { Refuse '027' 'Self-issued test or root certificate cannot sign a public release' }
if ($cert.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false) -cne $policy.publisherName) {
    Refuse '028' 'Authenticated publisher name differs from approved literal'
}
$eku = @($cert.Extensions | Where-Object { $_ -is [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] })
if ($eku.Count -ne 1 -or @($eku[0].EnhancedKeyUsages | Where-Object { $_.Value -eq '1.3.6.1.5.5.7.3.3' }).Count -ne 1) {
    Refuse '029' 'Publisher certificate lacks explicit Code Signing EKU'
}
$chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
try {
    $chain.ChainPolicy.RevocationMode = [Security.Cryptography.X509Certificates.X509RevocationMode]::Online
    $chain.ChainPolicy.RevocationFlag = [Security.Cryptography.X509Certificates.X509RevocationFlag]::EntireChain
    $chain.ChainPolicy.VerificationFlags = [Security.Cryptography.X509Certificates.X509VerificationFlags]::NoFlag
    $chain.ChainPolicy.UrlRetrievalTimeout = [TimeSpan]::FromSeconds(15)
    if (-not $chain.Build($cert) -or $chain.ChainElements.Count -lt 2 -or
        $chain.ChainElements[$chain.ChainElements.Count-1].Certificate.Thumbprint -cne $policy.trustedRootThumbprint) {
        Refuse '030' 'Publisher certificate does not have the approved revocation-checked public CA chain'
    }
} finally { $chain.Dispose() }
# This check NEVER proves timestamp countersignature, manifest signing, source
# reproducibility, or publisher's release approval. No executable is signed.
[pscustomobject]@{
    scope='LOCAL_CERTIFICATE_PREFLIGHT_ONLY_NOT_PRODUCTION_RELEASE'
    sourceCommit=$policy.sourceCommit
    publisherThumbprint=$policy.publisherCertificateThumbprint
    trustedRootThumbprint=$policy.trustedRootThumbprint
    timestampServiceConfigured=$true
    toolSha256=$toolHash
    actualArtifactSignatureVerified=$false
    timestampCountersignatureVerified=$false
    productionReleaseQualified=$false
} | ConvertTo-Json -Compress
