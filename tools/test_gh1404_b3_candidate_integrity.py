"""Mutation controls for a source-only B-3 archive/installed-lock receipt."""

import base64
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import zlib


SOURCE = Path(__file__).resolve().parents[1]
VERIFIER = SOURCE / "tools/gh1404_b3_candidate_integrity.py"
SELECTED = "8884fb212b2c2835d63d00d6021a24dc5a956f1a"

VERIFIER_SPEC = importlib.util.spec_from_file_location("b3_candidate_verifier", VERIFIER)
verifier = importlib.util.module_from_spec(VERIFIER_SPEC)
VERIFIER_SPEC.loader.exec_module(verifier)


def packed(name, version, dependencies, extras=None):
    files = {
        "package.json": (json.dumps({
            "name": name, "version": version, "dependencies": dependencies,
        }) + "\n").encode(),
        "entry.js": b"export const provenance = 'candidate';\n",
    }
    files.update(extras or {})
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for path, data in sorted(files.items()):
            info = tarfile.TarInfo("package/" + path)
            info.size = len(data)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
    return output.getvalue(), files


def raw_tar_header(name, kind=tarfile.REGTYPE, size=0, mode=0o644):
    entry = tarfile.TarInfo(name)
    entry.type = kind
    entry.size = size
    entry.mode = mode
    return entry.tobuf(tarfile.GNU_FORMAT)


def tar_pad(payload):
    return payload + bytes((-len(payload)) % 512)


def pax_record(field):
    length = len(field) + 3
    while True:
        record = str(length).encode("ascii") + b" " + field + b"\n"
        if len(record) == length:
            return record
        length = len(record)


class ArchiveBudgetTest(unittest.TestCase):
    def test_invisible_pax_headers_count_toward_physical_tar_limit(self):
        def make_archive(pax_count, visible_count):
            output = io.BytesIO()
            with tarfile.open(fileobj=output, mode="w:") as archive:
                for _ in range(pax_count):
                    metadata = tarfile.TarInfo("././@PaxHeader")
                    metadata.type = tarfile.XHDTYPE
                    metadata.mode = 0o644
                    metadata.size = 0
                    archive.addfile(metadata, io.BytesIO())
                manifest = b'{"name":"@keld/api","version":"0.0.1"}'
                info = tarfile.TarInfo("package/package.json")
                info.mode = 0o644
                info.size = len(manifest)
                archive.addfile(info, io.BytesIO(manifest))
                for index in range(visible_count - 1):
                    info = tarfile.TarInfo("package/entry" + str(index))
                    info.mode = 0o644
                    info.size = 1
                    archive.addfile(info, io.BytesIO(b"x"))
            return gzip.compress(output.getvalue())

        # tarfile consumes PAX metadata before returning a visible member;
        # a count of yielded members cannot bound all physical TAR headers.
        for hidden, visible in ((100, 901), (20, 981)):
            with self.subTest(hidden=hidden, visible=visible):
                with self.assertRaisesRegex(ValueError, "physical tar headers outside bound"):
                    verifier.archive_members(make_archive(hidden, visible), "fixture")
        # Exactly MAX_MEMBERS physical headers must still be accepted.
        for hidden, visible in ((100, 900), (0, 1000)):
            with self.subTest(hidden=hidden, visible=visible):
                files, _, count = verifier.archive_members(
                    make_archive(hidden, visible), "fixture",
                )
                self.assertEqual(visible, count)
                self.assertEqual(visible, len(files))

    def test_concat_second_gzip_stream_is_refused(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        for tail in (b"", b"UNACCOUNTED SECOND STREAM"):
            with self.subTest(tail=tail):
                with self.assertRaisesRegex(ValueError, "trailing gzip"):
                    verifier.archive_members(data + gzip.compress(tail), "fixture")

    def test_gzip_decoder_error_becomes_labeled_refusal(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        with mock.patch.object(zlib, "decompressobj", side_effect=zlib.error("decoder failure")):
            with self.assertRaisesRegex(ValueError, "^fixture: invalid gzip archive$"):
                verifier.archive_members(data, "fixture")

    def test_truncated_gzip_member_is_refused(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        for cut in (1, 8):
            with self.subTest(cut=cut):
                with self.assertRaisesRegex(ValueError, "incomplete gzip member"):
                    verifier.archive_members(data[:-cut], "fixture")

    def test_tar_recursion_at_open_and_iteration_becomes_labeled_refusal(self):
        data, _ = packed("@keld/api", "0.0.1", {})
        for after_first_header in (False, True):
            with self.subTest(after_first_header=after_first_header):
                parsed = 0
                next_member = tarfile.TarFile.next

                def fail_next(archive):
                    nonlocal parsed
                    if parsed >= int(after_first_header):
                        raise RecursionError("parser nesting limit")
                    parsed += 1
                    return next_member(archive)

                with mock.patch.object(tarfile.TarFile, "next", fail_next):
                    with self.assertRaisesRegex(ValueError, "^fixture: tar metadata nesting outside bound$"):
                        verifier.archive_members(data, "fixture")

    def test_inflated_tar_equality_passes_and_one_byte_over_refuses(self):
        data, expected = packed("@keld/api", "0.0.1", {})
        size = len(gzip.decompress(data))
        with mock.patch.object(verifier, "MAX_TAR_STREAM", size, create=True):
            files, directories, count = verifier.archive_members(data, "fixture")
        self.assertEqual({name: (body, 0o644) for name, body in expected.items()}, files)
        self.assertEqual({}, directories)
        self.assertEqual(2, count)
        with mock.patch.object(verifier, "MAX_TAR_STREAM", size - 1, create=True):
            with self.assertRaisesRegex(ValueError, "expanded tar stream outside bound"):
                verifier.archive_members(data, "fixture")

    def test_cumulative_budget_checked_before_extracting_next_member(self):
        data, expected = packed("@keld/api", "0.0.1", {}, {"z-note.txt": b"ordinary file content\n" * 20})
        extract = tarfile.TarFile.extractfile
        visited = []

        def record_extract(archive, member):
            visited.append(member.name)
            return extract(archive, member)

        limit = sum(map(len, expected.values()))
        with mock.patch.object(verifier, "MAX_ARCHIVE", limit):
            files, _, _ = verifier.archive_members(data, "fixture")
        self.assertEqual(set(expected), set(files))
        with mock.patch.object(verifier, "MAX_ARCHIVE", limit - 1):
            with mock.patch.object(tarfile.TarFile, "extractfile", record_extract):
                with self.assertRaisesRegex(ValueError, "^fixture: excessive expanded bytes$"):
                    verifier.archive_members(data, "fixture")
        self.assertEqual(["package/entry.js", "package/package.json"], visited)

    def test_archive_scan_stops_at_first_member_over_count(self):
        data, expected = packed("@keld/api", "0.0.1", {}, {"z-extra.js": b"export {};\n"})
        original = tarfile.TarFile.next
        visited = set()

        def observe_next(archive):
            member = original(archive)
            if member is not None:
                visited.add(member.name)
            return member

        with mock.patch.object(verifier, "MAX_MEMBERS", 1):
            with mock.patch.object(tarfile.TarFile, "next", observe_next):
                with self.assertRaisesRegex(ValueError, "archive member count outside bound"):
                    verifier.archive_members(data, "fixture")
        self.assertEqual({"package/" + name for name in sorted(expected)[:2]}, visited)


    def test_refuses_unaccounted_tar_tail_inside_one_gzip_member(self):
        archive, _ = packed("@keld/api", "0.0.1", {})
        plain = gzip.decompress(archive)
        second, _ = packed("@keld/api", "0.0.1", {}, {"hidden.txt": b"not inventoried"})
        suffixes = (
            ("arbitrary data", b"UNACCOUNTED_TRAILING_TAR_DATA"),
            ("second tar", gzip.decompress(second)),
        )
        for kind, extra in suffixes:
            with self.subTest(kind=kind):
                with self.assertRaisesRegex(ValueError, "trailing nonzero tar data"):
                    verifier.archive_members(gzip.compress(plain + extra, mtime=0), "fixture")

    def test_accepts_zero_tar_padding_and_pax_long_names(self):
        archive, expected = packed(
            "@keld/api", "0.0.1", {},
            {"long-" + "x" * 150 + ".js": b"export const ok = true"},
        )
        for padding in (b"", bytes(4096)):
            with self.subTest(extra_zero_bytes=len(padding)):
                files, _, _ = verifier.archive_members(
                    gzip.compress(gzip.decompress(archive) + padding, mtime=0),
                    "fixture",
                )
                self.assertEqual(set(expected), set(files))

    def test_accepts_actual_npm_pack_gzip_bytes_without_npm_runtime(self):
        # npm 11.20.0 pack --ignore-scripts --offline, minimal @keld/api 0.0.1
        # with package.json and app.js; persisted here to avoid CI npm access.
        encoded = (
            "H4sIAAAAAAAC/+2VvQoCMRCEr/YpQmo5N3p3giL4DpZiEc5V490lIYkiiO/u+ttoqaKSr1kY"
            "ptgtZtbKspJL7Ehr07VP3gIAFFnGnulnsi5LenkBQhQFkA8E9PqkQfIBNj5IR6u84EiC3eeP"
            "gDtrXGCl0T4w6cqV2uKkMRWyEQtug8NWEvlj7DX/10kdYPSH8y/yh/yL/OSP+X8/e65lg3zA"
            "xxXWc3oDirf5Fp1XRpMKKaSClDn60ikbLmqjtGpkfesLZiX59fLUIsGZmvwLVaPngym//BU+"
            "O8QeiUQika/iCBE/CWoADAAA"
        )
        archive = base64.b64decode(encoded)
        self.assertEqual(
            hashlib.sha256(archive).hexdigest(),
            "c2c367fb580a6fb96b9953e5f03765f8a0663f48792f7c733b41d8ba041866a4",
        )
        files, directories, count = verifier.archive_members(archive, "npm fixture")
        self.assertEqual((count, set(files), directories),
                         (2, {"package.json", "app.js"}, {}))

    def test_refuses_tar_missing_both_or_one_end_marker_blocks(self):
        archive, _ = packed("@keld/api", "0.0.1", {})
        tar_data = gzip.decompress(archive)
        with tarfile.open(fileobj=io.BytesIO(tar_data), mode="r:") as reader:
            reader.getmembers()
            logical_end = reader.offset
        for tail in (b"", bytes(512)):
            with self.subTest(trailing_zero_blocks=len(tail) // 512):
                with self.assertRaisesRegex(ValueError, "incomplete tar end marker"):
                    verifier.archive_members(
                        gzip.compress(tar_data[:logical_end] + tail, mtime=0),
                        "fixture",
                    )

    def test_refuses_nonzero_regular_member_padding(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        raw = (raw_tar_header("package/package.json", size=len(manifest)) +
               manifest + b"X" + bytes(511 - len(manifest)) + bytes(1024))
        with self.assertRaisesRegex(ValueError, "nonzero tar payload padding"):
            verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_refuses_nonzero_gnu_longname_metadata_padding(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        base = (raw_tar_header("package/package.json", size=len(manifest)) +
                tar_pad(manifest))
        long_name = b"package/" + b"n" * 140 + b"\0"
        raw = (base + raw_tar_header("././@LongLink", tarfile.GNUTYPE_LONGNAME,
                                     len(long_name)) +
               long_name + b"X" + bytes(511 - len(long_name)) +
               raw_tar_header("package/x") + bytes(1024))
        with self.assertRaisesRegex(ValueError, "nonzero tar payload padding"):
            verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_refuses_nonzero_pax_metadata_padding(self):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w:", format=tarfile.PAX_FORMAT) as tar:
            item = tarfile.TarInfo("package/" + "x" * 160)
            item.size = 1
            tar.addfile(item, io.BytesIO(b"x"))
        raw = bytearray(output.getvalue())
        self.assertEqual(raw[156:157], tarfile.XHDTYPE)
        pax_size = tarfile.nti(raw[124:136])
        self.assertLess(pax_size, 512)
        raw[512 + pax_size] = ord("X")
        with self.assertRaisesRegex(ValueError, "nonzero tar payload padding"):
            verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_refuses_gnu_sparse_extensions_before_tarfile(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        base = (raw_tar_header("package/package.json", size=len(manifest)) +
                tar_pad(manifest))
        sparse = bytearray(raw_tar_header("package/sparse", tarfile.GNUTYPE_SPARSE))
        sparse[482] = 1  # Old GNU sparse extended block flag.
        sparse[483:495] = b"00000000000\0"
        sparse[148:156] = b"        "
        sparse[148:156] = ("%06o\0 " % sum(sparse)).encode("ascii")
        extension = bytes(504) + b"\x01" + bytes(7)
        for blocks in (0, 5000):
            with self.subTest(extension_blocks=blocks):
                raw = base + bytes(sparse)
                if blocks:
                    raw += extension * (blocks - 1) + bytes(512) + bytes(1024)
                with self.assertRaisesRegex(ValueError, "GNU sparse members unsupported"):
                    verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_refuses_pax_size_override_that_changes_raw_tar_offsets(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        record = pax_record(b"size=" + str(len(manifest)).encode("ascii"))
        raw = (raw_tar_header("././@PaxHeader", tarfile.XGLTYPE, len(record)) +
               tar_pad(record) +
               raw_tar_header("package/package.json", size=512) +
               tar_pad(manifest) + bytes(1024))
        with self.assertRaisesRegex(ValueError, "PAX size override unsupported"):
            verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_refuses_root_directory_with_nonzero_declared_size(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        raw = (raw_tar_header("package/", tarfile.DIRTYPE, size=123, mode=0o755) +
               raw_tar_header("package/package.json", size=len(manifest)) +
               tar_pad(manifest) + bytes(1024))
        with self.assertRaisesRegex(ValueError, "root directory has data"):
            verifier.archive_members(gzip.compress(raw, mtime=0), "fixture")

    def test_healthy_gnu_longname_extended_header_and_path(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        long_name = b"package/" + b"n" * 140 + b"\0"
        raw = (raw_tar_header("package/package.json", size=len(manifest)) +
               tar_pad(manifest) +
               raw_tar_header("././@LongLink", tarfile.GNUTYPE_LONGNAME,
                              len(long_name)) + tar_pad(long_name) +
               raw_tar_header("package/x") + bytes(1024))
        files, _, count = verifier.archive_members(
            gzip.compress(raw, mtime=0), "fixture",
        )
        self.assertEqual((count, set(files)),
                         (2, {"package.json", "n" * 140}))


    def test_refuses_pax_masked_unsafe_physical_header_name(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        record = pax_record(b"path=package/package.json")
        for raw_name in ("../../outside.txt", "package/../outside.txt", "..\\..\\outside.txt"):
            with self.subTest(raw_name=raw_name):
                archive = (
                    raw_tar_header("././@PaxHeader", tarfile.XHDTYPE, len(record)) +
                    tar_pad(record) + raw_tar_header(raw_name, size=len(manifest)) +
                    tar_pad(manifest) + bytes(1024)
                )
                with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as parsed:
                    self.assertEqual([member.name for member in parsed], ["package/package.json"])
                with self.assertRaisesRegex(ValueError, "unsafe archive raw tar member path"):
                    verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")

    def test_refuses_gnu_longname_masked_unsafe_physical_header_name(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        long_name = b"package/package.json\0"
        for raw_name in ("../../outside.txt", "package/../outside.txt", "..\\..\\outside.txt"):
            with self.subTest(raw_name=raw_name):
                archive = (
                    raw_tar_header("././@LongLink", tarfile.GNUTYPE_LONGNAME,
                                   len(long_name)) + tar_pad(long_name) +
                    raw_tar_header(raw_name, size=len(manifest)) +
                    tar_pad(manifest) + bytes(1024)
                )
                with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as parsed:
                    self.assertEqual([member.name for member in parsed], ["package/package.json"])
                with self.assertRaisesRegex(ValueError, "unsafe archive raw tar member path"):
                    verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")

    def test_refuses_nonzero_raw_member_name_padding_and_reserved_header(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        original = raw_tar_header("package/package.json", size=len(manifest))
        mutations = (
            ("name-after-nul", len(b"package/package.json") + 1,
             b"UNACCOUNTED", "raw tar member name padding"),
            ("header-reserved", 500, b"UNACCOUNTED", "raw tar header reserved bytes"),
        )
        for label, offset, payload, message in mutations:
            with self.subTest(mutation=label):
                header = bytearray(original)
                self.assertEqual(header[offset:offset + len(payload)], bytes(len(payload)))
                header[offset:offset + len(payload)] = payload
                header[148:156] = b" " * 8
                header[148:156] = (f"{sum(header):06o}\0 ").encode("ascii")
                archive = bytes(header) + tar_pad(manifest) + bytes(1024)
                with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as parsed:
                    self.assertEqual([member.name for member in parsed], ["package/package.json"])
                with self.assertRaisesRegex(ValueError, message):
                    verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")

    def test_accepts_ustar_prefixed_nested_member_with_safe_raw_path(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        part = "x" * 75
        filename = "a" * 55 + ".js"
        nested = "package/" + part + "/" + filename
        info = tarfile.TarInfo(nested)
        info.size = 1
        info.mode = 0o644
        archive = (
            raw_tar_header("package/package.json", size=len(manifest)) +
            tar_pad(manifest) + info.tobuf(tarfile.USTAR_FORMAT) +
            tar_pad(b"x") + bytes(1024)
        )
        files, _, count = verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")
        self.assertEqual(count, 2)
        self.assertEqual(set(files), {"package.json", part + "/" + filename})


    def test_refuses_noncanonical_pax_and_gnu_metadata_raw_name(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        variants = (
            (tarfile.XHDTYPE, pax_record(b"path=package/package.json")),
            (tarfile.GNUTYPE_LONGNAME, b"package/package.json\0"),
        )
        for kind, payload in variants:
            for raw_name in ("../../outside-meta", "..\\..\\outside-meta"):
                with self.subTest(kind=kind, raw_name=raw_name):
                    archive = (
                        raw_tar_header(raw_name, kind, len(payload)) +
                        tar_pad(payload) +
                        raw_tar_header("package/package.json", size=len(manifest)) +
                        tar_pad(manifest) + bytes(1024)
                    )
                    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as parsed:
                        self.assertEqual([member.name for member in parsed], ["package/package.json"])
                    with self.assertRaisesRegex(ValueError, "unsafe archive raw tar metadata path"):
                        verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")

    def test_refuses_nonzero_metadata_header_name_padding_and_reserved_bytes(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        variants = (
            (tarfile.XHDTYPE, "././@PaxHeader", pax_record(b"path=package/package.json")),
            (tarfile.GNUTYPE_LONGNAME, "././@LongLink", b"package/package.json\0"),
        )
        for kind, canonical_name, payload in variants:
            original = raw_tar_header(canonical_name, kind, len(payload))
            for case, offset, bytes_added, message in (
                ("name-padding", len(canonical_name.encode()) + 1,
                 b"UNACCOUNTED", "raw tar member name padding"),
                ("reserved", 500, b"UNACCOUNTED", "raw tar header reserved bytes"),
            ):
                with self.subTest(kind=kind, case=case):
                    head = bytearray(original)
                    self.assertEqual(head[offset:offset+len(bytes_added)], bytes(len(bytes_added)))
                    head[offset:offset+len(bytes_added)] = bytes_added
                    head[148:156] = b" " * 8
                    head[148:156] = (f"{sum(head):06o}\0 ").encode("ascii")
                    archive = (
                        bytes(head) + tar_pad(payload) +
                        raw_tar_header("package/package.json", size=len(manifest)) +
                        tar_pad(manifest) + bytes(1024)
                    )
                    with self.assertRaisesRegex(ValueError, message):
                        verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")


    def test_refuses_hidden_gnu_longname_metadata_prefix(self):
        manifest = b'{"name":"@keld/api","version":"0.0.1"}'
        long_name = b"package/package.json\0"
        header = bytearray(raw_tar_header("././@LongLink", tarfile.GNUTYPE_LONGNAME,
                                          len(long_name)))
        self.assertEqual(header[345:355], bytes(10))
        header[345:355] = b"HIDDEN1234"
        header[148:156] = b" " * 8
        header[148:156] = (f"{sum(header):06o}\0 ").encode("ascii")
        archive = (bytes(header) + tar_pad(long_name) +
                   raw_tar_header("package/package.json", size=len(manifest)) +
                   tar_pad(manifest) + bytes(1024))
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as parsed:
            self.assertEqual([member.name for member in parsed], ["package/package.json"])
        with self.assertRaisesRegex(ValueError, "unsafe archive raw tar metadata path"):
            verifier.archive_members(gzip.compress(archive, mtime=0), "fixture")


    @staticmethod
    def _npm_11_long_nested_tgz():
        # Real npm 11.20.0 --offline --ignore-scripts TGZ, captured by W2.
        # Deliberately include raw PaxHeader/<basename> and equal size=24.
        encoded = (
            "H4sIAAAAAAAC/+2UT0+DMBiHe+ZTND0LvIXyLmzhvqN346HMuqFQCO2UaPzudpl6IDEz0WHQ"
            "Ppc3aZv+y/v8LuWwVvJG9XF5LggAoBCUjIEjIkVK0gyBc0Rw6yAFxIySgUzA3ljZu6t8d5/j"
            "W+hHnQlJhrSTdld0cnMvtyquqzKW03O+9vuc6M4EPKeNrRpViDznmGQAAefUVE+qSERA/jwT"
            "fPMp/53uI/8RkCeuztn/X/bpq9dWQ9f2lm5abSytW70tbL9Xq3/Q+Z4D7236Vl0ktvqnzzjl"
            "/0KM/Oc8E4uZ+z8TnpmWjWJLVqbhYxLqrgkPKcAu2IPqTdVqN8UjiMCN3Fa1Mmx5xVymsesX"
            "nxEej8czY14BsvI0AgAQAAA="
        )
        blob = base64.b64decode(encoded)
        assert hashlib.sha256(blob).hexdigest() == (
            "89300bd0fb81878d23c6a93730adc7a33996804826eaa01e959b532b3a4d766c"
        )
        return blob

    def test_accepts_exact_real_npm_11_nested_pax_name_and_equal_size(self):
        blob = self._npm_11_long_nested_tgz()
        plain = gzip.decompress(blob)
        raw_pax_header = tarfile.TarInfo.frombuf(plain[:512], "utf-8", "surrogateescape")
        raw_payload_header = tarfile.TarInfo.frombuf(
            plain[1024:1536], "utf-8", "surrogateescape"
        )
        self.assertEqual(raw_pax_header.type, tarfile.XHDTYPE)
        self.assertTrue(raw_pax_header.name.startswith("PaxHeader/"))
        self.assertIn(b"11 size=24\n", plain[512:512 + raw_pax_header.size])
        self.assertEqual(raw_payload_header.size, 24)
        files, _, count = verifier.archive_members(blob, "genuine npm long path")
        self.assertEqual((len(files), count), (2, 2))
        self.assertIn("package.json", files)
        self.assertIn("lib/" + "a" * 115 + "/" + "b" * 115 + ".js", files)
        self.assertEqual(len(next(value for name,value in files.items()
                                  if name.endswith(".js"))[0]), 24)

    def test_accepts_genuine_npm_11_utf8_pax_pseudoheader(self):
        # Real npm 11.20.0 offline pack of @keld/api@0.0.1 with lib/über.js.
        # npm emits a physical UTF-8 PaxHeader/<basename>, not just ASCII.
        encoded = (
            'H4sIAAAAAAAC/+2Vv07DMBCHM+cpTp6rxE4cRwFFYmRkrxjc9gCTP45st4pAvBkbL4aDUBcylkh'
            'R/S1n/eTB1vnzPcjxHuUBTfr1uUOTvNro4lBKBecwl/9Q5hDlhaCMCUH9Pr8ouM/GaAGO1knjj3'
            'KBS3rgXFdCVsEg3Us9yH0jnzFt1e78EmJWQedUhzWvKiaygtKYMbDqDeuMxVFg/cz1fWn/s+KP/'
            '2U+qRT8/3dwHLRxsNe9daAbqIHdBrOvz//f6u3X/eLzn8/M/zL4vwTvpJcdkhsgdw22h1QOimyA'
            'nNBYpfsppwlN2JQ9qRatT7bEDwvy+BG+iUAgEFgx3wO5kpQAEAAA'
        )
        blob = base64.b64decode(encoded)
        self.assertEqual(hashlib.sha256(blob).hexdigest(), "ce8f16a6143a651b3f96a4325d05198db1d9f24ea7b0f4d7cfadc25da2503be9")
        plain = gzip.decompress(blob)
        raw = tarfile.TarInfo.frombuf(plain[:512], "utf-8", "strict")
        self.assertEqual(raw.type, tarfile.XHDTYPE)
        self.assertEqual(raw.name, "PaxHeader/über.js")
        files, directories, count = verifier.archive_members(blob, "genuine npm UTF8")
        self.assertEqual((count, len(files)), (2, 2))
        self.assertIn("lib", directories)
        self.assertEqual(files["lib/über.js"][0], b"export const ok = 1;\n")

    def test_refuses_npm_pax_size_mismatch_on_either_header(self):
        plain = bytearray(gzip.decompress(self._npm_11_long_nested_tgz()))
        self.assertIn(b"11 size=24\n", plain)
        # Same-length replacement keeps TAR alignment intact but changes PAX claim.
        disagreeing_pax = bytes(plain).replace(b"11 size=24\n", b"11 size=25\n")
        self.assertEqual(len(disagreeing_pax), len(plain))
        with self.assertRaisesRegex(ValueError, "PAX size differs from physical tar header"):
            verifier.archive_members(gzip.compress(disagreeing_pax, mtime=0), "bad pax size")
        # Mutate the physical member's octal size independently and repair checksum.
        physical = bytearray(plain)
        position = 1024
        physical[position + 124:position + 136] = b"00000000031\0"  # octal 25.
        physical[position + 148:position + 156] = b"        "
        checksum = sum(physical[position:position + 512])
        physical[position + 148:position + 156] = (f"{checksum:06o}\0 ").encode("ascii")
        self.assertEqual(tarfile.TarInfo.frombuf(
            physical[position:position+512], "utf-8", "surrogateescape").size, 25)
        with self.assertRaisesRegex(ValueError, "PAX size differs from physical tar header"):
            verifier.archive_members(gzip.compress(physical, mtime=0), "bad physical size")

    def test_refuses_spoofed_npm_pax_pseudoname_and_hidden_unsafe_payload(self):
        original = gzip.decompress(self._npm_11_long_nested_tgz())
        source_pax = tarfile.TarInfo.frombuf(original[:512], "utf-8", "surrogateescape")
        self.assertTrue(source_pax.name.startswith("PaxHeader/"))
        for name in ("PaxHeader/../../outside", "PaxHeader/..\\outside",
                     "PaxHeader/wrong-basename", "PaxHeader/"):
            with self.subTest(pseudoname=name):
                replacement = bytearray(raw_tar_header(name, tarfile.XHDTYPE, source_pax.size))
                self.assertEqual(len(replacement), 512)
                mutated = bytes(replacement) + original[512:]
                with self.assertRaisesRegex(ValueError, "unsafe archive raw tar metadata path"):
                    verifier.archive_members(gzip.compress(mutated, mtime=0), "spoofed npm pax")
        physical = bytearray(original)
        regular_start = 1024
        unsafe = raw_tar_header("../../outside.js", size=24)
        physical[regular_start:regular_start+512] = unsafe
        with self.assertRaisesRegex(ValueError, "unsafe archive raw tar member path"):
            verifier.archive_members(gzip.compress(physical, mtime=0), "masked raw traversal")
        # Preserve the real npm physical basename so the PAX alias check
        # passes; the later file-kind admission itself must refuse a symlink.
        symlink = bytearray(original)
        symlink[regular_start + 156] = ord("2")  # TAR symlink member type.
        symlink[regular_start + 148:regular_start + 156] = b"        "
        check = sum(symlink[regular_start:regular_start + 512])
        symlink[regular_start + 148:regular_start + 156] = (
            f"{check:06o}\0 "
        ).encode("ascii")
        with self.assertRaisesRegex(ValueError, "non-regular archive member"):
            verifier.archive_members(gzip.compress(symlink, mtime=0), "symlink tgz")


    def test_refuses_unseen_bytes_in_real_npm_pax_and_member_padding(self):
        genuine = gzip.decompress(self._npm_11_long_nested_tgz())
        meta = tarfile.TarInfo.frombuf(genuine[:512], "utf-8", "surrogateescape")
        actual = tarfile.TarInfo.frombuf(genuine[1024:1536], "utf-8", "surrogateescape")
        self.assertEqual((meta.size, actual.size), (286, 24))
        variants = (
            ("metadata-hidden-padding", 512 + meta.size,
             "nonzero tar payload padding", False),
            ("member-hidden-padding", 1536 + actual.size,
             "nonzero tar payload padding", False),
            ("metadata-reserved", 500, "raw tar header reserved bytes", True),
        )
        for name, byte_position, expected, fix_checksum in variants:
            with self.subTest(mutation=name):
                tampered = bytearray(genuine)
                self.assertEqual(tampered[byte_position], 0)
                tampered[byte_position] = 0x41
                if fix_checksum:
                    tampered[148:156] = b" " * 8
                    total = sum(tampered[:512])
                    tampered[148:156] = (f"{total:06o}\0 ").encode("ascii")
                with self.assertRaisesRegex(ValueError, expected):
                    verifier.archive_members(gzip.compress(tampered, mtime=0), "hidden bytes")


    def test_refuses_npm_pax_safe_effective_name_spoof(self):
        # Both names stay below package/, but two TAR consumers would select
        # different physical files when the PAX path lies about the raw name.
        source = gzip.decompress(self._npm_11_long_nested_tgz())
        needle = b"path=package/lib/"
        self.assertEqual(source.count(needle), 1)
        spoofed = source.replace(needle, b"path=package/zzz/", 1)
        self.assertEqual(len(spoofed), len(source))
        with tarfile.open(fileobj=io.BytesIO(spoofed), mode="r:") as reader:
            self.assertTrue(next(iter(reader)).name.startswith("package/zzz/"))
        with self.assertRaisesRegex(ValueError, "npm PAX path disagrees with physical tar header"):
            verifier.archive_members(gzip.compress(spoofed, mtime=0), "safe effective spoof")


class CandidateIntegrityTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(
            prefix="gh1404-b3-integrity-", dir=os.path.realpath(tempfile.gettempdir()),
        )
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.project = root / "project"
        self.repro = root / "second"
        self.project.mkdir()
        self.repro.mkdir()
        self.source = SOURCE
        (self.project / "vendor").mkdir()
        source_manifest = SOURCE / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json"
        shutil.copyfile(source_manifest, self.project / "package.json")
        self.manifest = json.loads(source_manifest.read_text())
        version = {
            n: json.loads((SOURCE / "packages/@keld" / n / "package.json").read_text())["version"]
            for n in ("api", "kipc", "electron")
        }
        node = json.loads((SOURCE / "packages/@keld/electron/package.json").read_text())[
            "dependencies"
        ]["@types/node"]
        self.specs = [
            ("api", "node_modules/@keld/api", "@keld/api", version["api"],
             {"@keld/kipc": version["kipc"]}),
            ("kipc", "node_modules/@keld/kipc", "@keld/kipc", version["kipc"], {}),
            ("electron", "node_modules/electron", "@keld/electron", version["electron"],
             {"@keld/api": version["api"], "@keld/kipc": version["kipc"], "@types/node": node}),
            ("types-node", "node_modules/@types/node", "@types/node", node,
             {"undici-types": "~8.3.0"}),
            ("undici-types", "node_modules/undici-types", "undici-types", "8.3.0", {}),
        ]
        self.lock = {
            "name": self.manifest["name"],
            "version": self.manifest["version"],
            "lockfileVersion": 3,
            "requires": True,
            "packages": {"": {
                "name": self.manifest["name"], "version": self.manifest["version"],
                "dependencies": self.manifest["dependencies"],
            }},
        }
        for spec in self.specs:
            self._replace_package(spec)
        self._write_lock()

    def _replace_package(self, spec, extras=None):
        short, location, name, version, deps = spec
        prefix = "keld-" + short if short in {"api", "kipc", "electron"} else short
        filename = f"{prefix}-{version}.tgz"
        archive, entries = packed(name, version, deps, extras)
        (self.project / "vendor" / filename).write_bytes(archive)
        (self.repro / filename).write_bytes(archive)
        installed = self.project / location
        for relative, payload in entries.items():
            path = installed / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
            path.chmod(0o644)
        self.lock["packages"][location] = {
            "name": name,
            "version": version,
            "resolved": "file:vendor/" + filename,
            "integrity": "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode(),
            **({"dependencies": deps} if deps else {}),
        }

    def _write_lock(self):
        (self.project / "package-lock.json").write_text(json.dumps(self.lock, indent=2) + "\n")

    def _add_archive_directory(self, short, directory, mode=0o755, before_files=False):
        spec = next(row for row in self.specs if row[0] == short)
        filename = (
            ("keld-" + short if short in {"api", "kipc", "electron"} else short)
            + "-" + spec[3] + ".tgz"
        )
        before = (self.project / "vendor" / filename).read_bytes()
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(before), mode="r:gz") as source:
            with tarfile.open(fileobj=output, mode="w:gz") as archive:
                extra = tarfile.TarInfo("package/" + directory + "/")
                extra.type = tarfile.DIRTYPE
                extra.mode = mode
                if before_files:
                    archive.addfile(extra)
                for member in source.getmembers():
                    archive.addfile(member, source.extractfile(member) if member.isfile() else None)
                if not before_files:
                    archive.addfile(extra)
        packed_archive = output.getvalue()
        (self.project / "vendor" / filename).write_bytes(packed_archive)
        (self.repro / filename).write_bytes(packed_archive)
        self.lock["packages"][spec[1]]["integrity"] = (
            "sha512-" + base64.b64encode(hashlib.sha512(packed_archive).digest()).decode()
        )
        self._write_lock()

    def _mutate_source_after_first_check(self, relative, changed):
        self.isolated_selected_source()
        target = self.source / relative
        original_archive_members = verifier.archive_members
        mutated = False

        def change_during_archive_census(*args):
            nonlocal mutated
            if not mutated:
                target.write_bytes(changed(target.read_bytes()))
                mutated = True
            return original_archive_members(*args)

        with mock.patch.object(
            verifier, "archive_members", side_effect=change_during_archive_census
        ):
            with self.assertRaisesRegex(ValueError, "selected source"):
                verifier.validate(self.source, SELECTED, self.project, self.repro)
        self.assertTrue(mutated, "the test must change the real file after the first check")

    def check(self, expected_ok, message=""):
        result = subprocess.run(
            [sys.executable, str(VERIFIER), "--source-root", str(self.source),
             "--selected-revision", SELECTED, "--candidate-root", str(self.project),
             "--repro-dir", str(self.repro)],
            check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.assertEqual(result.returncode == 0, expected_ok, result.stderr + result.stdout)
        if message:
            self.assertIn(message, result.stderr + result.stdout)
        return result

    def isolated_selected_source(self):
        # A separate Git repository with shared read-only object storage avoids
        # registering or mutating another worker's Git worktree/index.
        checkout = Path(self.temp.name) / "immutable-source-fixture"
        commands = [
            ["git", "clone", "--shared", "--no-checkout", str(SOURCE), str(checkout)],
            ["git", "-C", str(checkout), "config", "maintenance.auto", "false"],
            ["git", "-C", str(checkout), "sparse-checkout", "init", "--cone"],
            ["git", "-C", str(checkout), "config", "maintenance.auto", "false"],
            [
                "git", "-C", str(checkout), "sparse-checkout", "set",
                "packages/@keld", "crates/keld-cli/tests/fixtures/gh1404-minimal-app",
            ],
            ["git", "-C", str(checkout), "checkout", "--detach", SELECTED],
        ]
        for command in commands:
            result = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.source = checkout

    def test_clone_disables_maintenance_before_first_sparse_git_operation(self):
        """Catch a removed first config even if the later census config stays."""
        checkout = Path(self.temp.name) / "immutable-source-fixture"
        original_run = subprocess.run
        inspected = []

        def verify_clone_order(command, *args, **kwargs):
            if (not inspected and command[:4] ==
                    ["git", "-C", str(checkout), "sparse-checkout"]):
                # This is the first Git operation after clone/config in the
                # fixture. A later maintenance setting cannot protect it.
                config = original_run(
                    ["git", "-C", str(checkout), "config", "--local", "--get",
                     "maintenance.auto"],
                    capture_output=True, text=True, check=False,
                )
                self.assertEqual(config.returncode, 0,
                                 "temporary clone must set maintenance.auto first")
                self.assertEqual(config.stdout.strip(), "false")
                inspected.append("sparse-first")
            return original_run(command, *args, **kwargs)

        with mock.patch.object(subprocess, "run", side_effect=verify_clone_order):
            self.isolated_selected_source()
        self.assertEqual(inspected, ["sparse-first"],
                         "did not inspect the first real sparse Git operation")
        selected = original_run(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        self.assertEqual(selected, SELECTED)

    def test_git_exec_child_closes_held_root_fd_and_retains_pinned_cwd(self):
        # Execute an actual Python interpreter as the would-be Git executable.
        # It must see EBADF for the passed source FD after exec, then run real
        # Git successfully from the same descriptor-pinned working directory.
        real_git = shutil.which("git")
        self.assertIsNotNone(real_git)
        original_run = subprocess.run
        physical_source = os.path.realpath(self.source)
        expected_head = original_run(
            [real_git, "-C", physical_source, "rev-parse", "HEAD"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
        probe = """
import errno
import os
import subprocess
import sys
source_fd = int(sys.argv[1])
try:
    os.fstat(source_fd)
except OSError as error:
    if error.errno != errno.EBADF:
        print("UNEXPECTED_ROOT_FD_ERROR=" + str(error), file=sys.stderr)
        sys.exit(74)
else:
    print("POST_EXEC_ROOT_FD_LEAK", file=sys.stderr)
    sys.exit(75)
if os.path.realpath(os.getcwd()) != sys.argv[2]:
    print("PINNED_SOURCE_CWD_CHANGED", file=sys.stderr)
    sys.exit(76)
git = subprocess.run(
    [sys.argv[3], *sys.argv[4:]], capture_output=True, text=True, check=False,
)
sys.stdout.write(git.stdout)
sys.stderr.write(git.stderr)
sys.exit(git.returncode)
"""
        seen_fds = []

        def probe_executed_git(argv, *args, **kwargs):
            if argv and argv[0] == "git" and "preexec_fn" in kwargs:
                passed = kwargs.get("pass_fds", ())
                self.assertEqual(len(passed), 1)
                seen_fds.append(passed[0])
                argv = [
                    sys.executable, "-c", probe, str(passed[0]),
                    physical_source, real_git, *argv[1:],
                ]
            return original_run(argv, *args, **kwargs)

        with mock.patch.object(verifier.subprocess, "run", side_effect=probe_executed_git):
            result = verifier._git_run(self.source, "rev-parse", "HEAD", text=True)
        self.assertEqual(len(seen_fds), 1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), expected_head)

    def test_git_pinned_cwd_accepts_detached_dotgit_file_worktree(self):
        self.isolated_selected_source()
        linked = Path(self.temp.name) / "detached-gitfile-source"
        original_run = subprocess.run
        add = original_run(
            ["git", "-C", str(self.source), "-c", "core.hooksPath=/dev/null",
             "worktree", "add", "--detach", "--no-checkout", str(linked), SELECTED],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(add.returncode, 0, add.stderr)
        self.addCleanup(
            original_run, ["git", "-C", str(self.source), "worktree", "remove",
                           "--force", str(linked)],
            capture_output=True, text=True, check=False,
        )
        self.assertTrue((linked / ".git").is_file())
        self.assertEqual(verifier.git(linked, "rev-parse", "HEAD"), SELECTED)
        selected = verifier.immutable_selected_bytes(
            linked, SELECTED, verifier.SELECTED_FILES[0],
        )
        self.assertEqual(
            selected,
            (self.source / verifier.SELECTED_FILES[0]).read_bytes(),
        )

    def test_refuses_symlink_ancestor_of_source_root_before_git(self):
        alias = Path(self.temp.name) / "source-parent-alias"
        alias.symlink_to(self.source.parent, target_is_directory=True)
        self.source = alias / self.source.name
        self.check(False, "unsafe directory root")

    def test_source_root_admission_precedes_any_git_execution(self):
        alias = Path(self.temp.name) / "source-preflight-alias"
        alias.symlink_to(self.source.parent, target_is_directory=True)
        with mock.patch.object(verifier, "git", side_effect=AssertionError("Git executed first")):
            with self.assertRaisesRegex(ValueError, "unsafe directory root"):
                verifier.validate(alias / self.source.name, SELECTED, self.project, self.repro)

    def test_refuses_symlink_ancestor_of_candidate_root(self):
        alias = Path(self.temp.name) / "candidate-parent-alias"
        alias.symlink_to(self.project.parent, target_is_directory=True)
        self.project = alias / self.project.name
        self.check(False, "unsafe directory root")

    def test_refuses_symlink_ancestor_of_repro_root(self):
        alias = Path(self.temp.name) / "repro-parent-alias"
        alias.symlink_to(self.repro.parent, target_is_directory=True)
        self.repro = alias / self.repro.name
        self.check(False, "unsafe directory root")

    def test_refuses_relative_parent_traversal_inside_candidate_root(self):
        parent = self.project.parent / "traversal-parent"
        parent.mkdir()
        self.project = parent / ".." / self.project.name
        self.check(False, "unsafe directory root")

    def test_source_root_rejects_parent_dotdot_lexically(self):
        source = self.source
        self.source = source.parent / ".." / source.parent.name / source.name
        self.check(False, "unsafe directory root")

    def test_repro_root_rejects_parent_dotdot_lexically(self):
        repro = self.repro
        self.repro = repro.parent / ".." / repro.parent.name / repro.name
        self.check(False, "unsafe directory root")

    def test_candidate_terminal_root_symlink_is_refused(self):
        alias = Path(self.temp.name) / "candidate-direct-alias"
        alias.symlink_to(self.project, target_is_directory=True)
        self.project = alias
        self.check(False, "unsafe directory root")

    def test_simple_relative_candidate_root_is_accepted(self):
        command = [
            sys.executable, str(VERIFIER), "--source-root", str(self.source),
            "--selected-revision", SELECTED, "--candidate-root", self.project.name,
            "--repro-dir", str(self.repro),
        ]
        result = subprocess.run(
            command, cwd=self.project.parent, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "UNQUALIFIED_SOURCE_ONLY")

    def test_root_component_open_rename_must_refuse_replacement(self):
        original_open = os.open
        root = self.project
        relocated = root.with_name("project-relocated")
        replaced = False

        def replace_after_open(path, flags, *args, **kwargs):
            nonlocal replaced
            fd = original_open(path, flags, *args, **kwargs)
            if not replaced and os.fspath(path) in {str(root), root.name}:
                replaced = True
                root.rename(relocated)
                root.symlink_to(relocated.name, target_is_directory=True)
            return fd

        with mock.patch.object(verifier.os, "open", side_effect=replace_after_open):
            with self.assertRaisesRegex(ValueError, "unsafe directory root"):
                fd = verifier.open_owned(root, "package.json", "candidate root swap")
                os.close(fd)
        self.assertTrue(replaced, "the actual root component must change after open")

    def test_root_rename_during_archive_inventory_is_refused(self):
        # Relocate an ancestor while its descendant root stays a real directory.
        # A terminal-only O_NOFOLLOW on the candidate root cannot see this.
        ancestor = Path(self.temp.name) / "candidate-live-parent"
        ancestor.mkdir()
        self.project.rename(ancestor / self.project.name)
        self.project = ancestor / self.project.name
        moved = ancestor.with_name("candidate-parent-moved")
        original_archive_members = verifier.archive_members
        changed = False

        def replace_during_archive(*args):
            nonlocal changed
            if not changed:
                changed = True
                ancestor.rename(moved)
                ancestor.symlink_to(moved.name, target_is_directory=True)
            return original_archive_members(*args)

        with mock.patch.object(verifier, "archive_members", side_effect=replace_during_archive):
            with self.assertRaisesRegex(ValueError, "unsafe directory root"):
                verifier.validate(self.source, SELECTED, self.project, self.repro)
        self.assertTrue(changed, "actual candidate ancestor must change during check")

    def test_absolute_physical_roots_preserve_source_only_success(self):
        self.source = Path(os.path.realpath(self.source))
        self.project = Path(os.path.realpath(self.project))
        self.repro = Path(os.path.realpath(self.repro))
        self.assertTrue(self.source.is_absolute() and self.project.is_absolute() and
                        self.repro.is_absolute())
        self.check(True)

    def test_refuses_uncommitted_post_selected_build_fixture(self):
        self.isolated_selected_source()
        changed = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/keld.build.ts"
        with changed.open("a", encoding="utf8") as writer:
            writer.write("\n// independently unapproved source modification\n")
        self.check(False, "selected source fixture")

    def test_refuses_matching_modified_source_and_candidate_package(self):
        self.isolated_selected_source()
        selected = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json"
        content = json.loads(selected.read_text("utf8"))
        content["reviewerUnapprovedField"] = "foreign"
        modified = json.dumps(content, indent=2) + "\n"
        selected.write_text(modified, encoding="utf8")
        (self.project / "package.json").write_text(modified, encoding="utf8")
        self.check(False, "selected source fixture")

    def test_refuses_mutation_of_selected_manifest_after_first_check(self):
        self._mutate_source_after_first_check(
            "crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json",
            lambda content: content + b"\n ",
        )

    def test_refuses_mutation_of_selected_build_after_first_check(self):
        self._mutate_source_after_first_check(
            "crates/keld-cli/tests/fixtures/gh1404-minimal-app/keld.build.ts",
            lambda content: content + b"\n// swapped after initial source check\n",
        )

    def test_refuses_selected_fixture_symlinked_parent(self):
        self.isolated_selected_source()
        fixture = self.source / "crates/keld-cli/tests/fixtures/gh1404-minimal-app"
        original = fixture.with_name(fixture.name + "-real")
        fixture.rename(original)
        fixture.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_candidate_vendor_symlinked_parent(self):
        vendor = self.project / "vendor"
        original = self.project / "vendor-real"
        vendor.rename(original)
        vendor.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_candidate_node_modules_symlinked_parent(self):
        installed = self.project / "node_modules"
        original = self.project / "node_modules-real"
        installed.rename(original)
        installed.symlink_to(original.name, target_is_directory=True)
        self.check(False, "directory")

    def test_refuses_installed_member_hardlink_to_uncustodied_path(self):
        installed = self.project / "node_modules/@keld/api/entry.js"
        os.link(installed, self.project / "unowned-hardlink.js")
        self.check(False, "hardlink")

    def test_refuses_archive_directory_missing_from_install(self):
        self._add_archive_directory("api", "orphan")
        self.check(False, "directories")

    def test_refuses_world_writable_archive_directory(self):
        self._add_archive_directory("api", "orphan", mode=0o777)
        self.check(False, "directory mode")

    def test_refuses_archive_file_directory_collision(self):
        self._add_archive_directory("api", "entry.js", before_files=True)
        self.check(False, "collision")

    def test_refuses_unlisted_installed_directory(self):
        (self.project / "node_modules/@keld/api/foreign-dir").mkdir()
        self.check(False, "directories")

    def test_refuses_foreign_top_level_installed_package(self):
        foreign = self.project / "node_modules/foreign-installed-package"
        foreign.mkdir()
        (foreign / "index.js").write_text("export const foreign = true;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_scoped_keld_provider(self):
        foreign = self.project / "node_modules/@keld/foreign-provider"
        foreign.mkdir()
        (foreign / "index.js").write_text("export const foreign = true;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_scoped_types_provider(self):
        foreign = self.project / "node_modules/@types/foreign-types"
        foreign.mkdir()
        (foreign / "index.d.ts").write_text("export type Foreign = string;\n")
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_symlinked_installed_package(self):
        outside = self.project / "external-package"
        outside.mkdir()
        (outside / "index.js").write_text("export const foreign = true;\n")
        (self.project / "node_modules/external-package").symlink_to(
            outside, target_is_directory=True,
        )
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_root_module_file(self):
        (self.project / "node_modules/foreign-module.js").write_text(
            "module.exports = 'unqualified';\n",
        )
        self.check(False, "unexpected installed package")

    def test_refuses_foreign_package_added_during_archive_check(self):
        original = verifier.archive_members
        injected = False

        def add_foreign_after_first_census(*args):
            nonlocal injected
            if not injected:
                injected = True
                foreign = self.project / "node_modules/foreign-installed-package"
                foreign.mkdir()
                (foreign / "index.js").write_text("export const foreign = true;\n")
            return original(*args)

        with mock.patch.object(verifier, "archive_members",
                               side_effect=add_foreign_after_first_census):
            with self.assertRaisesRegex(ValueError, "unexpected installed package"):
                verifier.validate(self.source, SELECTED, self.project, self.repro)
        self.assertTrue(injected, "the actual installed tree must change during validation")

    def test_allows_normal_npm_hidden_lock_file(self):
        (self.project / "node_modules/.package-lock.json").write_text("{}\n")
        self.check(True)

    def test_refuses_linked_hidden_npm_lock_file(self):
        outside = self.project / "external-package-lock.json"
        outside.write_text("{}\n")
        (self.project / "node_modules/.package-lock.json").symlink_to(outside)
        self.check(False, "file mode or metadata")

    def test_refuses_world_writable_node_modules_root(self):
        (self.project / "node_modules").chmod(0o777)
        self.check(False, "directory mode")

    def test_refuses_world_writable_installed_directory(self):
        (self.project / "node_modules/@keld/api").chmod(0o777)
        self.check(False, "directory mode")

    def test_accepts_matching_explicit_archive_directory_and_counts_entries(self):
        self._add_archive_directory("api", "empty")
        (self.project / "node_modules/@keld/api/empty").mkdir()
        receipt = json.loads(self.check(True).stdout)
        api = next(row for row in receipt["packages"] if row["name"] == "@keld/api")
        self.assertEqual(api["memberCount"], 3)

    def test_selected_source_receipts_equal_independent_git_blob_digests(self):
        receipt = json.loads(self.check(True).stdout)
        for path, field in (
            ("crates/keld-cli/tests/fixtures/gh1404-minimal-app/package.json",
             "selectedManifestSha256"),
            ("packages/@keld/kipc/scripts/package-artifacts.ts", "producerSourceSha256"),
        ):
            immutable = subprocess.run(
                ["git", "-C", str(SOURCE), "show", SELECTED + ":" + path],
                check=True, capture_output=True,
            ).stdout
            self.assertEqual(receipt[field], hashlib.sha256(immutable).hexdigest())

    def test_healthy_sealed_candidate(self):
        result = self.check(True)
        self.assertIn("UNQUALIFIED_SOURCE_ONLY", result.stdout)

    def test_core_provider_fingerprint_matches_pinned_two_file_vector(self):
        # Reference vector from the Core graph contract: SHA256 of each
        # sorted relative path + NUL + decimal size + NUL + raw SHA256 + NUL.
        # The two-file test KIPC fixture has no archive-local metadata noise.
        actual = json.loads(self.check(True).stdout)
        kipc = next(p for p in actual["packages"] if p["name"] == "@keld/kipc")
        self.assertEqual(
            kipc["candidateCoreUnpackedTreeSha256"],
            "3bff413379d590bd7a7267613a6a19d8d1ea3d87f4313bfe4c506e9a1fdad2b2",
        )
        self.assertEqual(actual["status"], "UNQUALIFIED_SOURCE_ONLY")

    def test_self_consistent_unapproved_archive_changes_candidate_core_fingerprint(self):
        baseline = json.loads(self.check(True).stdout)
        baseline_kipc = next(p for p in baseline["packages"] if p["name"] == "@keld/kipc")
        spec = next(row for row in self.specs if row[0] == "kipc")
        self._replace_package(
            spec,
            {"entry.js": b"export const provenance = 'unqualified variant';\n"},
        )
        self._write_lock()
        changed = json.loads(self.check(True).stdout)
        changed_kipc = next(p for p in changed["packages"] if p["name"] == "@keld/kipc")
        self.assertNotEqual(
            baseline_kipc["candidateCoreUnpackedTreeSha256"],
            changed_kipc["candidateCoreUnpackedTreeSha256"],
        )
        self.assertEqual(changed["status"], "UNQUALIFIED_SOURCE_ONLY")

    def test_rejects_changed_installed_file(self):
        (self.project / "node_modules/@keld/api/entry.js").write_text("modified\n")
        self.check(False, "installed")

    def test_rejects_changed_archive_despite_matching_repro(self):
        path = self.project / "vendor/keld-api-0.0.1.tgz"
        path.write_bytes(path.read_bytes() + b"X")
        shutil.copyfile(path, self.repro / path.name)
        self.check(False, "integrity")

    def test_rejects_changed_integrity_with_matching_files(self):
        row = self.lock["packages"]["node_modules/electron"]
        row["integrity"] = "sha512-" + base64.b64encode(b"\x00" * 64).decode()
        self._write_lock()
        self.check(False, "integrity")

    def test_rejects_lock_claim_of_install_script(self):
        self.lock["packages"]["node_modules/@keld/api"]["hasInstallScript"] = True
        self._write_lock()
        self.check(False, "install script")

    def test_rejects_source_version_substitution_despite_consistent_archive_and_lock(self):
        spec = next(row for row in self.specs if row[0] == "api")
        _, location, name, version, dependencies = spec
        self.assertEqual(version, self.manifest["dependencies"]["@keld/api"])
        other_version = next(row[3] for row in self.specs if row[0] == "types-node")
        archive, entries = packed(name, other_version, dependencies)
        filename = f"keld-api-{version}.tgz"
        (self.project / "vendor" / filename).write_bytes(archive)
        (self.repro / filename).write_bytes(archive)
        for relative, payload in entries.items():
            (self.project / location / relative).write_bytes(payload)
        self.lock["packages"][location]["version"] = other_version
        self.lock["packages"][location]["integrity"] = (
            "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode()
        )
        self._write_lock()
        self.check(False, "source version")

    def test_rejects_mode_substitution(self):
        (self.project / "node_modules/@keld/kipc/entry.js").chmod(0o600)
        self.check(False, "mode")

    def test_rejects_extra_installed_provider_member(self):
        (self.project / "node_modules/@keld/kipc/foreign.js").write_text("foreign\n")
        self.check(False, "unexpected")

    def test_rejects_test_hook_even_when_every_digest_agrees(self):
        spec = next(row for row in self.specs if row[0] == "kipc")
        self._replace_package(spec, {"src/test-hooks.ts": b"export const hook = 1;\n"})
        self._write_lock()
        self.check(False, "test")

    def test_rejects_backslash_archive_member_even_when_installed_matches(self):
        spec = next(row for row in self.specs if row[0] == "api")
        self._replace_package(spec, {"..\\escape.js": b"export const escape = 1;\n"})
        self._write_lock()
        self.check(False, "unsafe archive")

    def test_rejects_cross_cwd_archive_difference(self):
        path = self.repro / "keld-electron-0.0.1.tgz"
        path.write_bytes(path.read_bytes() + b"X")
        self.check(False, "reproduc")

    def test_rejects_alias_redirection(self):
        manifest = json.loads((self.project / "package.json").read_text())
        manifest["dependencies"]["electron"] = "electron@0.0.1"
        (self.project / "package.json").write_text(json.dumps(manifest))
        self.lock["packages"][""]["dependencies"] = manifest["dependencies"]
        self._write_lock()
        self.check(False, "source")


class GitEnvironmentIsolationTest(unittest.TestCase):
    def test_clean_source_filter_from_local_git_attributes_never_executes(self):
        # Git status can run an executable filter while still reporting CLEAN.
        # These local attributes are not selected source inputs and must not run.
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-git-clean-filter-", dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            path = source / "packages/@keld/api/package.json"
            path.parent.mkdir(parents=True)
            content = b'{"name":"@keld/api","version":"0.0.1"}\n'
            path.write_bytes(content)

            def run_git(*argv):
                done = subprocess.run(["git", *argv], capture_output=True,
                                      text=True, check=False)
                self.assertEqual(done.returncode, 0, done.stderr)
                return done.stdout.strip()

            run_git("init", "-q", str(source))
            run_git("-C", str(source), "config", "maintenance.auto", "false")
            run_git("-C", str(source), "add", "packages/@keld/api/package.json")
            run_git("-C", str(source), "-c", "user.name=B3 fixture",
                    "-c", "user.email=b3@example.invalid",
                    "-c", "commit.gpgsign=false", "commit", "-qm", "selected")
            selected = run_git("-C", str(source), "rev-parse", "HEAD")
            marker = root / "clean-filter-executed"
            cleaner = root / "clean-filter.sh"
            cleaner.write_text("#!/bin/sh\nprintf ran >> " + str(marker) + "\ncat\n")
            cleaner.chmod(0o700)
            attributes = source / ".git/info/attributes"
            attributes.write_text("packages/@keld/api/package.json filter=untrusted\n")
            run_git("-C", str(source), "config", "filter.untrusted.clean", str(cleaner))
            run_git("-C", str(source), "config", "filter.untrusted.required", "true")
            current = path.stat()
            os.utime(path, ns=(current.st_atime_ns, current.st_mtime_ns + 3_000_000_000))
            raw = verifier._git_run(
                source, "status", "--porcelain", "--", "packages/@keld", text=True,
            )
            self.assertEqual(raw.returncode, 0, raw.stderr)
            self.assertEqual(raw.stdout, "", "unchanged physical bytes must be clean")
            self.assertTrue(marker.exists(), "raw Git status must exercise the hostile fixture")
            marker.unlink()
            verifier.selected_source_packages_unchanged(source, selected)
            self.assertFalse(marker.exists(), "source observer executed untrusted clean filter")
            path.write_bytes(content + b"// changed\n")
            with self.assertRaisesRegex(ValueError, "source package files have unpublished edits"):
                verifier.selected_source_packages_unchanged(source, selected)
            self.assertFalse(marker.exists(), "dirty-source check ran untrusted clean filter")

    def test_repository_external_diff_helper_cannot_run_during_source_check(self):
        # Git can run local diff.external during `git diff --quiet` when the
        # repository enables diff.trustExitCode. The verifier must not execute
        # such a helper, even when changed committed provider source is refused.
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-git-external-diff-",
            dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            parent = Path(temporary)
            source = parent / "source"
            source.mkdir()
            helper = parent / "external-diff.sh"
            witness = parent / "diff-external-executed"
            helper.write_text("#!/bin/sh\necho invoked > " + str(witness) + "\nexit 1\n")
            helper.chmod(0o700)

            def run_git(*argv):
                done = subprocess.run(["git", *argv], capture_output=True,
                                      text=True, check=False)
                self.assertEqual(done.returncode, 0, done.stderr)
                return done.stdout.strip()

            run_git("init", "-q", str(source))
            run_git("-C", str(source), "config", "maintenance.auto", "false")
            path = source / "packages/@keld/check.txt"
            path.parent.mkdir(parents=True)
            path.write_text("selected original\n")
            run_git("-C", str(source), "add", "packages/@keld/check.txt")
            commit = ["-c", "user.name=B3 fixture", "-c", "user.email=b3@example.invalid",
                      "-c", "commit.gpgsign=false", "commit", "-qm"]
            run_git("-C", str(source), *commit, "selected")
            selected = run_git("-C", str(source), "rev-parse", "HEAD")
            path.write_text("different committed source\n")
            run_git("-C", str(source), "add", "packages/@keld/check.txt")
            run_git("-C", str(source), *commit, "changed")
            run_git("-C", str(source), "config", "diff.external", str(helper))
            run_git("-C", str(source), "config", "diff.trustExitCode", "true")
            # The ordinary Git invocation can execute this configured helper.
            uncontrolled = verifier._git_run(
                source, "diff", "--quiet", selected, "HEAD", "--", "packages/@keld",
                text=True,
            )
            self.assertNotEqual(uncontrolled.returncode, 0)
            if witness.exists():
                witness.unlink()
            # The actual admission check must refuse changed source without
            # executing the helper. A clean source is checked by other tests.
            with verifier.RootCustody(source, "source root") as pinned:
                with self.assertRaisesRegex(ValueError, "source Git identity or tree mismatch"):
                    verifier._validate_owned(pinned, selected, pinned, pinned)
            self.assertFalse(witness.exists(), "external Git diff helper executed")

    def test_rebound_git_worktree_alias_cannot_hide_extra_physical_source(self):
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-core-worktree-rebind-",
            dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            parent = Path(temporary)
            source = parent / "source"
            foreign = parent / "foreign"
            alias = parent / "alias"
            source.mkdir()
            file = source / "packages/@keld/api/package.json"
            file.parent.mkdir(parents=True)
            file.write_text('{"name":"@keld/api","version":"0.0.1"}\n')

            def run_git(*args):
                done = subprocess.run(["git", *args], capture_output=True,
                                      text=True, check=False)
                self.assertEqual(done.returncode, 0, done.stderr)
                return done.stdout.strip()

            run_git("init", "-q", str(source))
            run_git("-C", str(source), "config", "maintenance.auto", "false")
            run_git("-C", str(source), "add", "packages/@keld/api/package.json")
            run_git("-C", str(source), "-c", "user.name=B3 fixture",
                    "-c", "user.email=b3@example.invalid",
                    "-c", "commit.gpgsign=false", "commit", "-qm", "selected")
            selected = run_git("-C", str(source), "rev-parse", "HEAD")
            run_git("clone", "-q", str(source), str(foreign))
            alias.symlink_to(source, target_is_directory=True)
            run_git("-C", str(source), "config", "core.worktree", str(alias))
            verifier.require_pinned_git_worktree(source)
            verifier.selected_source_packages_unchanged(source, selected)
            # Ignore rules and an external worktree alias must never hide an
            # extra physical input in the first-party source tree.
            extra = source / "packages/@keld/ignored.js"
            extra.write_text("unselected source\n")
            (source / ".git/info/exclude").write_text("packages/@keld/ignored.js\n")
            alias.unlink()
            alias.symlink_to(foreign, target_is_directory=True)
            self.assertFalse(verifier._git_run(
                source, "ls-files", "--others", "--exclude-standard", "-z",
                "--", "packages/@keld", text=False,
            ).stdout, "foreign clean Git tree must demonstrate the false-clean case")
            with self.assertRaisesRegex(ValueError, "extra or absent physical provider source"):
                verifier.selected_source_packages_unchanged(source, selected)

    def test_local_git_core_worktree_cannot_redirect_source_cleanliness(self):
        # A repository-local override can point the source Git status at a
        # different clean tree even when all inherited GIT_* inputs are scrubbed.
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-git-local-", dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            parent = Path(temporary)
            source = parent / "source"
            clean = parent / "clean"
            source.mkdir()

            def run_git(*argv):
                completed = subprocess.run(
                    ["git", *argv], capture_output=True, text=True, check=False,
                )
                self.assertEqual(completed.returncode, 0, completed.stderr)
                return completed.stdout.strip()

            run_git("init", "-q", str(source))
            run_git("-C", str(source), "config", "maintenance.auto", "false")
            (source / "payload.txt").write_text("selected original\n")
            run_git("-C", str(source), "add", "payload.txt")
            run_git("-C", str(source), "-c", "user.name=B3 fixture",
                    "-c", "user.email=b3@example.invalid",
                    "-c", "commit.gpgsign=false", "commit", "-qm", "selected")
            run_git("clone", "-q", str(source), str(clean))
            (source / "payload.txt").write_text("uncommitted changed source\n")
            verifier.require_pinned_git_worktree(source)  # Clean Git identity control.
            self.assertIn("payload.txt", verifier.git(source, "status", "--porcelain"))
            run_git("-C", str(source), "config", "core.worktree", str(clean))
            # Prove the original false-green mechanism and require its refusal.
            self.assertEqual(verifier.git(source, "status", "--porcelain"), "")
            with self.assertRaisesRegex(ValueError, "source Git worktree differs"):
                verifier.require_pinned_git_worktree(source)

    def test_configured_git_fsmonitor_does_not_execute_before_source_admission(self):
        # A configured helper can run during git status if core.fsmonitor is
        # inherited. That must not execute from an unqualified source root.
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-fsmonitor-", dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            parent = Path(temporary)
            source = parent / "source"
            source.mkdir()
            helper = parent / "fsmonitor.sh"
            witness = parent / "fsmonitor-executed"
            helper.write_text("#!/bin/sh\necho invoked > " + str(witness) + "\nexit 0\n")
            helper.chmod(0o700)
            init = subprocess.run(["git", "init", "-q", str(source)],
                                  capture_output=True, text=True, check=False)
            self.assertEqual(init.returncode, 0, init.stderr)
            subprocess.run(
                ["git", "-C", str(source), "config", "maintenance.auto", "false"],
                capture_output=True, text=True, check=True,
            )
            config = subprocess.run(
                ["git", "-C", str(source), "config", "core.fsmonitor", str(helper)],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(config.returncode, 0, config.stderr)
            (source / "payload.txt").write_text("uncommitted input\n")
            actual = verifier.git(source, "status", "--porcelain", "--", "payload.txt")
            self.assertIn("?? payload.txt", actual)
            self.assertFalse(witness.exists(), "Git executed a configured fsmonitor")

    def test_git_environment_cannot_redirect_pinned_source(self):
        with tempfile.TemporaryDirectory(
            prefix="gh1404-b3-foreign-git-", dir=os.path.realpath(tempfile.gettempdir()),
        ) as temporary:
            source = Path(temporary) / "source"
            clean = Path(temporary) / "clean-foreign"
            source.mkdir()
            baseline_env = {
                key: value for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }

            def run_git(*args):
                result = subprocess.run(
                    ["git", *args], env=baseline_env, text=True,
                    capture_output=True, check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

            run_git("init", "-q", str(source))
            run_git("-C", str(source), "config", "maintenance.auto", "false")
            (source / "payload.txt").write_text("selected original\n")
            run_git("-C", str(source), "add", "payload.txt")
            run_git(
                "-C", str(source), "-c", "user.name=B3 fixture",
                "-c", "user.email=b3@example.invalid",
                "-c", "commit.gpgsign=false", "commit", "-qm", "selected",
            )
            run_git("clone", "-q", str(source), str(clean))
            (source / "payload.txt").write_text("uncommitted changed source\n")
            baseline = verifier.git(source, "status", "--porcelain", "--", "payload.txt")
            self.assertIn("payload.txt", baseline)
            injections = (
                {"GIT_WORK_TREE": str(clean)},
                {"GIT_DIR": str(clean / ".git"), "GIT_WORK_TREE": str(clean)},
                {
                    "GIT_CONFIG_COUNT": "1",
                    "GIT_CONFIG_KEY_0": "core.worktree",
                    "GIT_CONFIG_VALUE_0": str(clean),
                },
            )
            for injection in injections:
                with self.subTest(env=tuple(sorted(injection))):
                    with mock.patch.dict(os.environ, injection, clear=False):
                        result = verifier.git(
                            source, "status", "--porcelain", "--", "payload.txt",
                        )
                    self.assertIn(
                        "payload.txt", result,
                        "ambient Git configuration hid an uncommitted source mutation",
                    )


if __name__ == "__main__":
    unittest.main()
