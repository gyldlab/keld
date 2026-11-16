# In-process DER parser for the developer-local signer preflight.
# Hash the original SubjectPublicKeyInfo TLV; do not start an external runtime.

function Refuse-Der([string]$Reason) {
    throw "KELD-DEVELOPER-SIGNER-PREFLIGHT: $Reason"
}

function Read-KeldDerTlv([byte[]]$Data, [int]$Offset, [int]$Limit) {
    if ($Offset -lt 0 -or $Offset + 2 -gt $Limit) {
        Refuse-Der 'certificate DER header is truncated'
    }
    $tag = [int]$Data[$Offset]
    $first = [int]$Data[$Offset + 1]
    $body = $Offset + 2
    if (($first -band 0x80) -ne 0) {
        $count = $first -band 0x7f
        if ($count -eq 0 -or $count -gt 3 -or $body + $count -gt $Limit) {
            Refuse-Der 'certificate DER length is invalid'
        }
        if ($Data[$body] -eq 0) { Refuse-Der 'certificate DER length is noncanonical' }
        $length = 0
        for ($index = 0; $index -lt $count; $index++) {
            $length = ($length -shl 8) -bor [int]$Data[$body + $index]
        }
        if ($length -lt 128) { Refuse-Der 'certificate DER long length is noncanonical' }
        $body += $count
    } else {
        $length = $first
    }
    $end = $body + $length
    if ($end -gt $Limit) { Refuse-Der 'certificate DER value exceeds its parent' }
    return [pscustomobject]@{ Tag = $tag; Body = $body; End = $end; Next = $end }
}

function Get-KeldSubjectPublicKeyInfo([byte[]]$CertificateBytes) {
    if ($null -eq $CertificateBytes -or $CertificateBytes.Length -eq 0 -or
        $CertificateBytes.Length -gt 16384) {
        Refuse-Der 'certificate DER size is outside the supported bound'
    }
    $outer = Read-KeldDerTlv $CertificateBytes 0 $CertificateBytes.Length
    if ($outer.Tag -ne 0x30 -or $outer.Next -ne $CertificateBytes.Length) {
        Refuse-Der 'certificate DER sequence is invalid'
    }
    $tbs = Read-KeldDerTlv $CertificateBytes $outer.Body $outer.End
    if ($tbs.Tag -ne 0x30) { Refuse-Der 'certificate TBSCertificate is invalid' }
    $signatureAlgorithm = Read-KeldDerTlv $CertificateBytes $tbs.Next $outer.End
    if ($signatureAlgorithm.Tag -ne 0x30) { Refuse-Der 'certificate signature algorithm is missing' }
    $signature = Read-KeldDerTlv $CertificateBytes $signatureAlgorithm.Next $outer.End
    if ($signature.Tag -ne 0x03 -or $signature.Body -eq $signature.End -or
        $signature.Next -ne $outer.End) {
        Refuse-Der 'certificate signature value is invalid'
    }

    $position = $tbs.Body
    if ($position -lt $tbs.End -and $CertificateBytes[$position] -eq 0xa0) {
        $version = Read-KeldDerTlv $CertificateBytes $position $tbs.End
        $position = $version.Next
    }
    foreach ($expectedTag in @(0x02, 0x30, 0x30, 0x30, 0x30)) {
        $field = Read-KeldDerTlv $CertificateBytes $position $tbs.End
        if ($field.Tag -ne $expectedTag) {
            Refuse-Der 'certificate TBSCertificate field order is invalid'
        }
        $position = $field.Next
    }

    $spkiStart = $position
    $spki = Read-KeldDerTlv $CertificateBytes $spkiStart $tbs.End
    if ($spki.Tag -ne 0x30) { Refuse-Der 'certificate SubjectPublicKeyInfo is missing' }
    $algorithm = Read-KeldDerTlv $CertificateBytes $spki.Body $spki.End
    if ($algorithm.Tag -ne 0x30) { Refuse-Der 'certificate SPKI algorithm is missing' }
    $publicKey = Read-KeldDerTlv $CertificateBytes $algorithm.Next $spki.End
    if ($publicKey.Tag -ne 0x03 -or $publicKey.Body -eq $publicKey.End -or
        $publicKey.Next -ne $spki.End) {
        Refuse-Der 'certificate SPKI public key is invalid'
    }
    $unusedBits = [int]$CertificateBytes[$publicKey.Body]
    if ($unusedBits -gt 7 -or
        ($publicKey.End - $publicKey.Body -eq 1 -and $unusedBits -ne 0)) {
        Refuse-Der 'certificate SPKI unused-bit count is invalid'
    }

    $spkiBytes = New-Object byte[] ($spki.End - $spkiStart)
    [Array]::Copy($CertificateBytes, $spkiStart, $spkiBytes, 0, $spkiBytes.Length)
    return ,$spkiBytes
}

function Get-KeldPublisherScope([byte[]]$CertificateBytes) {
    $spkiBytes = Get-KeldSubjectPublicKeyInfo $CertificateBytes
    $domain = [Text.Encoding]::ASCII.GetBytes('keld.publisher.windows/v1')
    $inputBytes = New-Object byte[] ($domain.Length + 1 + $spkiBytes.Length)
    [Array]::Copy($domain, 0, $inputBytes, 0, $domain.Length)
    $inputBytes[$domain.Length] = 0
    [Array]::Copy($spkiBytes, 0, $inputBytes, $domain.Length + 1, $spkiBytes.Length)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($inputBytes)).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha256.Dispose()
    }
}
