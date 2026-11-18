#!/usr/bin/env python3
"""Independent reference oracle for the PowerShell SPKI parser tests.

The input is base64-encoded public certificate DER on stdin. No private key,
certificate store, signer, or trust policy is accessed or changed here.
Production signer preflight parses certificate DER in-process and never runs
this module or another Python executable.
"""
import base64
import hashlib
import sys

DOMAIN = b"keld.publisher.windows/v1\x00"
MAX_CERTIFICATE_BYTES = 16384


def tlv(data: bytes, offset: int, limit: int) -> tuple[int, int, int, int]:
    if offset + 2 > limit:
        raise ValueError("truncated DER")
    tag = data[offset]
    first = data[offset + 1]
    pos = offset + 2
    if first & 0x80:
        count = first & 0x7F
        if count == 0 or count > 3 or pos + count > limit:
            raise ValueError("invalid DER length")
        if data[pos] == 0:
            raise ValueError("noncanonical DER length")
        length = int.from_bytes(data[pos : pos + count], "big")
        if length < 128:
            raise ValueError("noncanonical DER long length")
        pos += count
    else:
        length = first
    end = pos + length
    if end > limit:
        raise ValueError("DER value exceeds parent")
    return tag, pos, end, end


def publisher_scope(cert: bytes) -> str:
    if not cert or len(cert) > MAX_CERTIFICATE_BYTES:
        raise ValueError("certificate size")
    tag, cert_body, cert_end, finish = tlv(cert, 0, len(cert))
    if tag != 0x30 or finish != len(cert):
        raise ValueError("invalid certificate sequence")
    tag, tbs_body, tbs_end, after_tbs = tlv(cert, cert_body, cert_end)
    if tag != 0x30:
        raise ValueError("invalid TBSCertificate")
    tag, _, _, after_algorithm = tlv(cert, after_tbs, cert_end)
    if tag != 0x30:
        raise ValueError("missing certificate signature algorithm")
    tag, value_start, value_end, after_signature = tlv(cert, after_algorithm, cert_end)
    if tag != 0x03 or value_end == value_start or after_signature != cert_end:
        raise ValueError("invalid certificate signature value")
    pos = tbs_body
    if pos < tbs_end and cert[pos] == 0xA0:  # optional explicit version
        _, _, _, pos = tlv(cert, pos, tbs_end)
    for expected in (0x02, 0x30, 0x30, 0x30, 0x30):
        tag, _, _, pos = tlv(cert, pos, tbs_end)
        if tag != expected:
            raise ValueError("unexpected TBSCertificate field")
    spki_start = pos
    tag, spki_body, spki_end, pos = tlv(cert, pos, tbs_end)
    if tag != 0x30:
        raise ValueError("missing SubjectPublicKeyInfo")
    tag, _, _, inside = tlv(cert, spki_body, spki_end)
    if tag != 0x30:
        raise ValueError("missing SPKI algorithm")
    tag, bit_start, bit_end, inside = tlv(cert, inside, spki_end)
    if tag != 0x03 or inside != spki_end or bit_start == bit_end:
        raise ValueError("invalid SPKI public key")
    if cert[bit_start] > 7 or (bit_end - bit_start == 1 and cert[bit_start] != 0):
        raise ValueError("invalid SPKI unused bits")
    # The parser deliberately keeps the original certificate's encoded SPKI
    # bytes. Reconstructing the key from a .NET object can change its DER.
    return hashlib.sha256(DOMAIN + cert[spki_start:pos]).hexdigest()


def main() -> int:
    try:
        encoded = sys.stdin.buffer.read(MAX_CERTIFICATE_BYTES * 2 + 1).strip()
        if len(encoded) > MAX_CERTIFICATE_BYTES * 2:
            raise ValueError("oversized certificate input")
        der = base64.b64decode(encoded, validate=True)
        print(publisher_scope(der))
        return 0
    except (ValueError, OverflowError):
        print("SPKI_INPUT_REJECTED", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
