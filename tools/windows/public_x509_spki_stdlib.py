# KEL-96 developer-public-certificate SPKI derivation; stdlib only, no private-key access.
# Input: one base64 DER public X.509 certificate on stdin. Output: one lowercase SHA-256 scope.
import base64
import hashlib
import sys

def die(reason):
    raise SystemExit("FAIL_CLOSED: " + reason)

def der_tlv(data, offset):
    if offset >= len(data):
        die("truncated DER tag")
    start = offset
    tag = data[offset]
    offset += 1
    if offset >= len(data):
        die("truncated DER length")
    length_octet = data[offset]
    offset += 1
    if length_octet & 0x80:
        count = length_octet & 0x7f
        if count == 0 or count > 4 or offset + count > len(data):
            die("invalid DER length")
        if data[offset] == 0:
            die("nonminimal DER length")
        length = int.from_bytes(data[offset:offset + count], "big")
        if length < 128:
            die("nonminimal DER length")
        offset += count
    else:
        length = length_octet
    end = offset + length
    if end > len(data):
        die("DER exceeds input")
    return tag, start, offset, end

def expect(data, offset, tag):
    found, start, value_start, end = der_tlv(data, offset)
    if found != tag:
        die("unexpected DER tag")
    return start, value_start, end

payload = sys.stdin.read(262145).strip()
if not payload or len(payload) > 262144:
    die("missing or oversized base64 input")
try:
    certificate = base64.b64decode(payload, validate=True)
except Exception:
    die("invalid base64")
if len(certificate) > 131072:
    die("oversized certificate")
_, outer_body, outer_end = expect(certificate, 0, 0x30)
if outer_end != len(certificate):
    die("trailing certificate bytes")
_, tbs_body, tbs_end = expect(certificate, outer_body, 0x30)
cursor = tbs_body
if der_tlv(certificate, cursor)[0] == 0xa0:  # [0] EXPLICIT version, optional
    cursor = der_tlv(certificate, cursor)[3]
for field_tag in (0x02, 0x30, 0x30, 0x30, 0x30):  # serial, signature, issuer, validity, subject
    _, _, cursor = expect(certificate, cursor, field_tag)
spki_start, spki_body, spki_end = expect(certificate, cursor, 0x30)
if spki_end > tbs_end:
    die("SPKI outside TBSCertificate")
_, _, algorithm_end = expect(certificate, spki_body, 0x30)
_, _, key_end = expect(certificate, algorithm_end, 0x03)
if key_end != spki_end:
    die("invalid SPKI structure")
spki = certificate[spki_start:spki_end]
if not (1 <= len(spki) <= 65536):
    die("invalid SPKI length")
print(hashlib.sha256(b"keld.publisher.windows/v1\x00" + spki).hexdigest())
