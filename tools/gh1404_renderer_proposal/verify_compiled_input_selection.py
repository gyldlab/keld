#!/usr/bin/env python3
"""Pinned original Ant source-input proof. Never approves a compiled release."""

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import xml.etree.ElementTree as ET


class Refusal(ValueError):
    """An unqualified original / selected-source / compiled input."""


PINS = {
    "pinned-drawio-recursive-tree.json": "65d5ff12629a950efac059a27c5476ce23c5ffecaa7c09b97365624cd961bfbb",
    "drawio-mxgraph-src-mxClient.js": "04cd816916e71a175ef7e6cd9ab49b7ea27e642e846ec56331dc1979a801acdf",
    "drawio-mxgraph-COMPILED-ORIGINAL-mxClient.js": "c8f121c2e4bb0a33294052eca9d8cf5ddb94526b95b64464345766577863289b",
    "drawio-pinned-build.xml": "1a07ef97c97cf8c73093548de7d87aa80bad5cdf4002f80437c5af3b4bed2f22",
    "drawio-pinned-build.properties": "6b0820dba6a76aa31a3d611ed70b601746e531d0d72fb734c1241d1f67813575",
    "G1-candidate-mxSvgCanvas2D.js": "838862ae4d8f8af0a0dc1d4acb1f9fa7613afbab23419e8cbacf7cb61c0328b3",
    "G1-candidate-mxText.js": "c67a67cc2d0ae435baee216ead31c672236cff79eec38cd84de8d8bf6138658e",
}
UPSTREAM = "24b76c2cbd55d88e354042e8d329a2e4708972bc"
GIT_TREE = "dded4c5aaecbc5a7fb87e92d9363b80a36329488"
SRC_MX_GIT = "c69218edaf6e4caf82db206afe260cacaf42929e"
OLD_MX_GIT = "d2611e23f620b19889201b44ef1d194fd3a69471"
APP_MIN_GIT = "ac88c96641c45fdffb3c64aca92d3329308b2ecf"
ANT_XML_GIT = "26111ae2536ef1e013b36c6244bb50c73ded613f"
ANT_PROPS_GIT = "23001e15b5a9e0fd3199840099eb7e6b67b7ab2c"
CLOSURE_GIT = "bb85b3f0e0e1628f2b21e7c04bdf707d66aba625"
XML2JS_GIT = "5c43289eb508213a2a82c17423843021b1e433c7"
STAGE_SHA = "be5db6590ce45599af2d646d5d69fe65ef5a02644fd933a41cf2e03c9ef63515"
PATCH_SHA = "d31486a5d7d1cbcfaec2aedd9309bb3d44e1b0efc06d11fb915164a98eb457e5"
PREFIX = "src/main/webapp/mxgraph/src"
OVERRIDES = {
    34: ("util/mxSvgCanvas2D.js",
         "0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3",
         "G1-candidate-mxSvgCanvas2D.js"),
    49: ("shape/mxText.js",
         "a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772",
         "G1-candidate-mxText.js"),
}


def require(ok, message):
    if not ok:
        raise Refusal(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git_blob(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def physical(path, *, evidence=None):
    """Read one regular inode through a no-follow walk of every path component.

    Leaf-only O_NOFOLLOW is insufficient: a swapped ancestor directory can
    otherwise redirect a byte-identical source outside the selected root.
    Keep the directory handles alive and rewalk the full name before success.
    """
    path = Path(os.path.abspath(path))
    require(path.is_absolute() and path.name not in ("", ".", ".."),
            "unsafe physical input path")
    directory_flags = (os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW |
                       getattr(os, "O_CLOEXEC", 0))
    file_flags = (os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK |
                  getattr(os, "O_CLOEXEC", 0))
    require(bool(os.O_DIRECTORY and os.O_NOFOLLOW), "no-follow filesystem required")

    def identity(info):
        return (info.st_dev, info.st_ino, info.st_size, info.st_mode,
                info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)

    with ExitStack() as stack:
        root_fd = os.open(path.anchor, directory_flags)
        stack.callback(os.close, root_fd)
        parent_fd = root_fd
        retained_directories = []
        for part in path.parts[1:-1]:
            opened = os.open(part, directory_flags, dir_fd=parent_fd)
            stack.callback(os.close, opened)
            retained_directories.append(opened)
            parent_fd = opened
        fd = os.open(path.name, file_flags, dir_fd=parent_fd)
        stack.callback(os.close, fd)
        start = os.fstat(fd)
        require(stat.S_ISREG(start.st_mode) and start.st_nlink == 1 and
                0 < start.st_size <= 8 * 1024 * 1024, "unsafe source inode: " + path.name)
        data = bytearray()
        while len(data) <= 8 * 1024 * 1024:
            part = os.read(fd, min(65536, 8 * 1024 * 1024 + 1 - len(data)))
            if not part:
                break
            data.extend(part)
        require(len(data) == start.st_size and identity(start) == identity(os.fstat(fd)),
                "source path or content changed while reading: " + path.name)

        # The second walk starts at the original anchored root, not at a
        # retained parent which might have been renamed in the meantime.
        live_parent = root_fd
        for part, retained in zip(path.parts[1:-1], retained_directories):
            live_dir = os.open(part, directory_flags, dir_fd=live_parent)
            stack.callback(os.close, live_dir)
            pinned_stat, live_stat = os.fstat(retained), os.fstat(live_dir)
            require((pinned_stat.st_dev, pinned_stat.st_ino) ==
                    (live_stat.st_dev, live_stat.st_ino),
                    "source parent directory changed: " + path.name)
            live_parent = live_dir
        live_fd = os.open(path.name, file_flags, dir_fd=live_parent)
        stack.callback(os.close, live_fd)
        require(identity(start) == identity(os.fstat(live_fd)) and
                identity(start) == identity(os.fstat(fd)),
                "source path or content changed while reading: " + path.name)
        selected_bytes = bytes(data)
        if evidence is not None:
            evidence.append((path, identity(start), selected_bytes))
        return selected_bytes


def pinned(root, file, *, evidence=None):
    require(file in PINS, "unlisted source input: " + file)
    data = physical(root / file, evidence=evidence)
    require(sha(data) == PINS[file], "immutable input digest mismatch: " + file)
    return data


def assert_ant(xml, props):
    app = [n for n in ET.fromstring(xml).iter("target") if n.get("name") == "app"]
    require(len(app) == 1 and app[0].get("depends") == "merge",
            "pinned Ant app target is absent or changed")
    compile_nodes = [n for n in app[0].iter("jscomp")
                     if n.get("output") == "${basedir}/client.min.js"]
    require(len(compile_nodes) == 1, "Ant client.min.js compiler selection changed")
    selected = [(s.get("dir"), f.get("name"))
                for s in compile_nodes[0].iter("sources") for f in s.iter("file")]
    require(("${war.dir}/mxgraph", "mxClient.js") in selected,
            "Ant must compile the PRECOMPILED mxgraph/mxClient.js")
    require(not any(name and name.endswith(("mxSvgCanvas2D.js", "mxText.js", "src/mxClient.js"))
                    for _dir, name in selected), "Ant unexpectedly selected raw G1 leaf")
    final = [n for n in app[0].iter("concat")
             if n.get("destfile") == "${war.dir}/js/app.min.js"]
    require(len(final) == 1 and any(
        "client.min.js" in f.get("files", "").split(",")
        for f in final[0].iter("filelist")
    ), "Ant app.min does not consume client.min")
    require(b"jscompiler=${basedir}/compiler.jar" in props,
            "default Closure JAR location changed")


def inspect(inputs, staged, original, *, candidate_precompiled=None, require_compiled=False):
    inputs, staged, original = [Path(v).resolve(strict=True) for v in (inputs, staged, original)]
    # One output receipt must describe one still-current physical selection,
    # not a mixture of names observed before and after a source-root swap.
    selected_evidence = []
    tree = json.loads(pinned(inputs, "pinned-drawio-recursive-tree.json",
                             evidence=selected_evidence))
    require(tree.get("sha") == GIT_TREE and tree.get("truncated") is False,
            "immutable upstream tree hash or truncation differs")
    blobs = {entry["path"]: entry["sha"] for entry in tree["tree"]
             if entry.get("type") == "blob"}
    for path, value in (
        ("src/main/webapp/mxgraph/src/mxClient.js", SRC_MX_GIT),
        ("src/main/webapp/mxgraph/mxClient.js", OLD_MX_GIT),
        ("src/main/webapp/js/app.min.js", APP_MIN_GIT),
        ("etc/build/build.xml", ANT_XML_GIT),
        ("etc/build/build.properties", ANT_PROPS_GIT),
        ("etc/build/compiler.jar", CLOSURE_GIT),
        ("etc/build/Xml2Js.class", XML2JS_GIT),
    ):
        require(blobs.get(path) == value, "immutable original Git tree mismatch: " + path)
    includes = pinned(inputs, "drawio-mxgraph-src-mxClient.js", evidence=selected_evidence)
    old_compiled = pinned(inputs, "drawio-mxgraph-COMPILED-ORIGINAL-mxClient.js",
                          evidence=selected_evidence)
    ant = pinned(inputs, "drawio-pinned-build.xml", evidence=selected_evidence)
    properties = pinned(inputs, "drawio-pinned-build.properties", evidence=selected_evidence)
    require(git_blob(includes) == SRC_MX_GIT and git_blob(old_compiled) == OLD_MX_GIT
            and git_blob(ant) == ANT_XML_GIT and git_blob(properties) == ANT_PROPS_GIT,
            "original source/compiled/Ant immutable Git blobs differ")
    assert_ant(ant, properties)
    refs = re.findall(
        r"mxClient\.include\(mxClient\.basePath \+ ['\"](/[^'\"]+)['\"]\);",
        includes.decode("utf8"),
    )
    require(len(refs) == 131 and len(set(refs)) == 131,
            "immutable 131 source includes missing/duplicated/reordered")
    stage_bytes = physical(staged / "source-manifest.json", evidence=selected_evidence)
    require(sha(stage_bytes) == STAGE_SHA, "source stage immutable manifest digest mismatch")
    stage = json.loads(stage_bytes)
    require(stage.get("upstream") == "jgraph/drawio@" + UPSTREAM and
            stage.get("patch_sha256") == PATCH_SHA and
            sha(physical(staged / "gh1404-g1-production.patch",
                         evidence=selected_evidence)) == PATCH_SHA,
            "source stage upstream or patch differs")
    require(len(stage.get("files", [])) == 2, "source stage has extra/missing G1 leaves")
    selected = []
    physically_verified = 0
    missing_originals = []
    for index, relative in enumerate(refs, 1):
        require(relative.startswith("/") and ".." not in relative.split("/") and
                "//" not in relative and "\\" not in relative, "unsafe mxGraph include name")
        logical = PREFIX + relative
        original_blob = blobs.get(logical)
        require(bool(original_blob) and re.fullmatch("[0-9a-f]{40}", original_blob),
                "immutable source include not selected: " + logical)
        path = original / logical
        if path.exists() or path.is_symlink():
            require(git_blob(physical(path, evidence=selected_evidence)) == original_blob,
                    "physical original source differs: " + logical)
            physically_verified += 1
        else:
            missing_originals.append(logical)
        changed_blob = original_blob
        if index in OVERRIDES:
            name, original_sha, owner_file = OVERRIDES[index]
            require(relative == "/" + name, "G1 source include order differs at " + str(index))
            require(path.is_file() and
                    sha(physical(path, evidence=selected_evidence)) == original_sha,
                    "missing exact pristine G1 original")
            candidate = pinned(inputs, owner_file, evidence=selected_evidence)
            require(physical(staged / logical, evidence=selected_evidence) == candidate,
                    "source stage changed G1 candidate bytes")
            rows = [r for r in stage["files"] if r.get("path") == logical]
            require(len(rows) == 1 and rows[0].get("original_sha256") == original_sha
                    and rows[0].get("candidate_sha256") == sha(candidate),
                    "source stage input/output hash receipt differs")
            changed_blob = git_blob(candidate)
        selected.append((index, logical, original_blob, changed_blob))
    require([index for index, _, before, after in selected if before != after] == [34, 49],
            "G1 has more or fewer than exactly two changed source includes")
    require(2 <= physically_verified <= 131,
            "required two physical G1 originals absent from selected source root")
    ordered = hashlib.sha256(b"keld.gh1404-g1-ordered-source-selection/v1\0")
    for index, path, _old, chosen in selected:
        ordered.update(str(index).encode() + b"\0" + path.encode() + b"\0" +
                       bytes.fromhex(chosen) + b"\0")
    if candidate_precompiled is not None:
        try:
            candidate = physical(Path(candidate_precompiled))
        except OSError as error:
            # An optional compiled candidate is never self-authorizing. In
            # particular, do not resolve an untrusted /var or source-parent
            # symlink just to print a more specific comparison result.
            raise Refusal("COMPILED_PROVENANCE_UNQUALIFIED: unsafe candidate input name") from error
        if sha(candidate) == PINS["drawio-mxgraph-COMPILED-ORIGINAL-mxClient.js"]:
            raise Refusal("PRISTINE_PRECOMPILED_REUSED: raw G1 leaves never reach the Ant input")
        raise Refusal("COMPILED_PROVENANCE_UNQUALIFIED: missing physical 131-source precompile, "
                      "selected Closure JAR/JDK/Ant and independently reviewed compiled receipt")
    if require_compiled:
        raise Refusal("COMPILED_PROVENANCE_UNQUALIFIED: no exact candidate precompiled "
                      "mxClient.js and independent 131-source/toolchain receipt")
    # A hostile writer can replace an already-read directory after the last
    # individual no-follow verification and before this receipt is returned.
    # Reopen every selected name from the filesystem root, rechecking bytes
    # AND held inode identity. A later same-UID edit after process exit still
    # requires independent build-consumer custody and cannot be approved here.
    for path, original_identity, original_bytes in selected_evidence:
        reread_evidence = []
        current_bytes = physical(path, evidence=reread_evidence)
        require(current_bytes == original_bytes and
                len(reread_evidence) == 1 and
                reread_evidence[0][1] == original_identity,
                "selected physical input changed before source receipt: " + path.name)
    return {
        "schema": "keld.gh1404-g1-compiled-input-custody/v1",
        "status": "SOURCE_ONLY_INPUT_ORDER_VERIFIED_COMPILED_UNQUALIFIED",
        "officialSourceCommit": UPSTREAM,
        "officialSourceRootTreeGitBlob": GIT_TREE,
        "officialMxGraphIncludeGitBlob": SRC_MX_GIT,
        "officialAntBuildXmlGitBlob": ANT_XML_GIT,
        "officialAntBuildPropertiesGitBlob": ANT_PROPS_GIT,
        "officialClosureCompilerJarGitBlob": CLOSURE_GIT,
        "officialXml2JsClassGitBlob": XML2JS_GIT,
        "originalPrecompiledMxClientGitBlob": OLD_MX_GIT,
        "originalAppMinGitBlob": APP_MIN_GIT,
        "selectedG1PatchSha256": PATCH_SHA,
        "selectedG1SourceStageManifestSha256": STAGE_SHA,
        "includeCount": 131,
        "patchPositions": [34, 49],
        "metadataSourceOrderDigestSha256": ordered.hexdigest(),
        "orderedSelectedSourceGitBlobs": [
            {"sequence": index, "path": path, "originalGitBlob": before,
             "selectedGitBlob": after, "G1SourcePatch": index in OVERRIDES}
            for index, path, before, after in selected
        ],
        "physicalSelectedSourceFiles": 2,
        "physicallyVerifiedOriginalFiles": physically_verified,
        "unverifiedPhysicalOriginalFiles": len(missing_originals),
        "missingOriginalSourcePaths": missing_originals,
        "antChain": "mxgraph/mxClient.js -> client.min.js -> js/app.min.js",
        "originalPrecompiledMxClientSha256": PINS["drawio-mxgraph-COMPILED-ORIGINAL-mxClient.js"],
        "missingQualifiedToolchain": ["JDK", "Ant", "Closure compiler.jar", "precompiler assembly"],
        "qualifiedClosureCompilerJarSha256": None,
        "qualifiedJdkIdentity": None,
        "qualifiedAntIdentity": None,
        "physical131InputStreamSha256": None,
        "buildPropertiesLocalOverrideAbsent": None,
        "rebuiltCompiledMxClientQualified": False,
        "candidateCompiledMxClientSha256": None,
        "candidateAppMinSha256": None,
        "signedOriginalAppQualified": False,
        "note": "Git tree blob identities for missing source files are metadata only, not physical source bytes; "
                "this receipt cannot prove the compiled Ant input or signed app.",
    }


def main():
    arg = argparse.ArgumentParser(description=__doc__)
    arg.add_argument("--owner-inputs", required=True, type=Path)
    arg.add_argument("--g1-stage", required=True, type=Path)
    arg.add_argument("--pristine-two-source-root", required=True, type=Path)
    arg.add_argument("--candidate-precompiled", type=Path)
    arg.add_argument("--require-compiled-proof", action="store_true")
    arg.add_argument("--output", type=Path)
    args = arg.parse_args()
    try:
        report = inspect(args.owner_inputs, args.g1_stage, args.pristine_two_source_root,
                         candidate_precompiled=args.candidate_precompiled,
                         require_compiled=args.require_compiled_proof)
        text = json.dumps(report, sort_keys=True, indent=2) + "\n"
        if args.output:
            with args.output.open("x", encoding="utf8") as dst:
                dst.write(text)
        print(text, end="")
        return 0
    except (Refusal, OSError, ValueError, ET.ParseError) as error:
        print("GH1404_G1_COMPILED_INPUT_REFUSED: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
