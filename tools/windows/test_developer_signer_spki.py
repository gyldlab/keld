"""Independent DER shape and KELD domain-separation tests (stdlib only)."""
import hashlib
import unittest

from developer_signer_spki import publisher_scope

EXPECTED_PUBLISHER_SCOPE_DOMAIN = b"keld.publisher.windows/v1\x00"


def der(tag: int, content: bytes) -> bytes:
    n = len(content)
    length = bytes((n,)) if n < 128 else b"\x81" + bytes((n,))
    return bytes((tag,)) + length + content


def certificate(spki: bytes) -> bytes:
    tbs = der(
        0x30,
        der(0xA0, der(0x02, b"\x02"))
        + der(0x02, b"\x01")
        + der(0x30, b"")  # signature algorithm
        + der(0x30, b"")  # issuer
        + der(0x30, b"")  # validity
        + der(0x30, b"")  # subject
        + spki,
    )
    return der(0x30, tbs + der(0x30, b"") + der(0x03, b"\x00\xaa"))


class PublisherScopeTests(unittest.TestCase):
    def test_exact_original_spki_der_and_domain(self):
        spki = der(0x30, der(0x30, der(0x06, b"\x2b\x65\x70")) + der(0x03, b"\x00\x01\x02"))
        self.assertEqual(
            publisher_scope(certificate(spki)),
            hashlib.sha256(EXPECTED_PUBLISHER_SCOPE_DOMAIN + spki).hexdigest(),
        )
        self.assertNotEqual(publisher_scope(certificate(spki)), hashlib.sha256(spki).hexdigest())

    def test_malformed_or_truncated_certificate_fails(self):
        good_spki = der(0x30, der(0x30, b"") + der(0x03, b"\x00\x01"))
        source = certificate(good_spki)
        for malformed in (source[:-1], source + b"\x00", source[:1] + b"\x81" + source[1:]):
            with self.subTest(malformed=malformed[-5:]), self.assertRaises(ValueError):
                publisher_scope(malformed)

    def test_wrong_subject_public_key_tag_fails(self):
        invalid_spki = der(0x30, der(0x30, b"") + der(0x04, b"\x00\x01"))
        with self.assertRaises(ValueError):
            publisher_scope(certificate(invalid_spki))

    def test_missing_subject_public_key_fails(self):
        with self.assertRaises(ValueError):
            publisher_scope(certificate(der(0x30, b"")))


if __name__ == "__main__":
    unittest.main()
