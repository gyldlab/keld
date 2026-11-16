# KEL-96 read-only local signer onboarding. Place beside public_x509_spki_stdlib.py.
# DISCOVER outputs public certificate identity only; VALIDATE performs the complete metadata preflight.
# Neither mode signs, installs, trusts, creates, exports or modifies certificate/private-key material.
[CmdletBinding(DefaultParameterSetName='Validate')]
param(
  [Parameter(Mandatory=$true)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$Thumbprint,
  [Parameter(Mandatory=$true,ParameterSetName='Discover')][switch]$DiscoverPublisherScope,
  [Parameter(Mandatory=$true,ParameterSetName='Validate')][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedPublisherScope,
  [Parameter(Mandatory=$true,ParameterSetName='Validate')][string]$SignToolPath,
  [Parameter(Mandatory=$true,ParameterSetName='Validate')][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedSignToolSha256,
  [Parameter(Mandatory=$true,ParameterSetName='Validate')][string]$ReleasePublicKeyPath,
  [string]$PythonExecutable='python'
)
$ErrorActionPreference='Stop'
$helper=Join-Path -Path $PSScriptRoot -ChildPath 'public_x509_spki_stdlib.py'
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) { throw 'FAIL_CLOSED: colocated public-SPKI helper missing' }
$pythonCommand=Get-Command -Name $PythonExecutable -CommandType Application -ErrorAction Stop | Select-Object -First 1
if ($null -eq $pythonCommand -or -not (Test-Path -LiteralPath $pythonCommand.Source -PathType Leaf)) {
  throw 'FAIL_CLOSED: Python interpreter is not an existing executable file'
}
$cert=Get-Item -LiteralPath ('Cert:\CurrentUser\My\' + $Thumbprint) -ErrorAction Stop
$now=Get-Date
if ($cert.Thumbprint -ne $Thumbprint.ToUpperInvariant()) { throw 'FAIL_CLOSED: certificate thumbprint mismatch' }
if (-not $cert.HasPrivateKey) { throw 'FAIL_CLOSED: certificate has no associated private key' }
if ($cert.NotBefore -gt $now -or $cert.NotAfter -le $now) { throw 'FAIL_CLOSED: certificate outside validity window' }
$eku=@($cert.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' })
if ($eku.Count -ne 1) { throw 'FAIL_CLOSED: one Code Signing EKU extension required' }
$ekuOids=@($eku[0].EnhancedKeyUsages | ForEach-Object { $_.Value })
if ($ekuOids -notcontains '1.3.6.1.5.5.7.3.3') { throw 'FAIL_CLOSED: certificate not authorized for Code Signing EKU' }
$usage=@($cert.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.15' })
if ($usage.Count -ne 1) { throw 'FAIL_CLOSED: one Digital Signature key usage extension required' }
$flag=[System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature
if (($usage[0].KeyUsages -band $flag) -ne $flag) { throw 'FAIL_CLOSED: Digital Signature key usage missing' }
# Export PUBLIC certificate DER; never PFX, private key or a trust store entry.
$publicDer=$cert.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
$scopeResult=@([Convert]::ToBase64String($publicDer) | & $pythonCommand.Source $helper)
if ($LASTEXITCODE -ne 0 -or $scopeResult.Count -ne 1 -or $scopeResult[0] -notmatch '^[a-f0-9]{64}$') {
  throw 'FAIL_CLOSED: public-SPKI scope calculation failed'
}
$scope=[string]$scopeResult[0]
if ($PSCmdlet.ParameterSetName -eq 'Discover') {
  Write-Output ('DISCOVERY_ONLY publisherScope=' + $scope)
  Write-Output ('certificateThumbprint=' + $cert.Thumbprint)
  Write-Output ('validThrough=' + $cert.NotAfter.ToString('o'))
  Write-Output 'NOT_PREFLIGHT_PASS: copy publisherScope into your own keld.build.ts and then run full Validate mode'
  return
}
if ($scope -ne $ExpectedPublisherScope.ToLowerInvariant()) { throw ('FAIL_CLOSED: publisherScope mismatch; derived=' + $scope) }
if (-not (Test-Path -LiteralPath $SignToolPath -PathType Leaf)) { throw 'FAIL_CLOSED: SignTool path absent' }
$toolHash=(Get-FileHash -LiteralPath $SignToolPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($toolHash -ne $ExpectedSignToolSha256.ToLowerInvariant()) { throw 'FAIL_CLOSED: SignTool SHA256 mismatch' }
if (-not (Test-Path -LiteralPath $ReleasePublicKeyPath -PathType Leaf)) { throw 'FAIL_CLOSED: existing release public key absent' }
$keyLength=(Get-Item -LiteralPath $ReleasePublicKeyPath).Length
if ($keyLength -ne 32) { throw 'FAIL_CLOSED: raw update public key size must be 32 bytes' }
Write-Output ('PREFLIGHT_PASS certificateThumbprint=' + $cert.Thumbprint)
Write-Output ('publisherScope=' + $scope)
Write-Output ('signToolSHA256=' + $toolHash)
Write-Output ('releasePublicKeyBytes=' + $keyLength)
Write-Output ('validThrough=' + $cert.NotAfter.ToString('o'))
Write-Output 'LIMIT: public metadata only; no actual key-sign operation, trust assertion or executable launch performed'
