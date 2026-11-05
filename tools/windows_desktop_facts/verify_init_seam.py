#!/usr/bin/env python3
"""Compile exact patched first-Init helper bodies with deterministic clock/send instrumentation.
No native APIs or Core admission execute. Existing owner claims and native gates still apply.
"""
from pathlib import Path
import argparse, datetime, hashlib, json, os, re, shutil, subprocess, time

def digest(data):
    return hashlib.sha256(data).hexdigest()

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--repo", type=Path, required=True)
    p.add_argument("--source-ref", required=True, help="immutable Git blob or COMMIT:path")
    p.add_argument("--expected-source-sha256", required=True)
    p.add_argument("--patch", type=Path, required=True)
    p.add_argument("--red-patch", type=Path, required=True)
    p.add_argument("--harness", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    for field in ("repo", "patch", "red_patch", "harness", "out"):
        setattr(args, field, getattr(args, field).resolve())
    def git_read(*words):
        return subprocess.check_output(["git", *words], cwd=args.repo)
    source = git_read("cat-file", "blob", args.source_ref)
    assert digest(source) == args.expected_source_sha256, "source bytes do not match the reviewed pin"
    blob = git_read("rev-parse", args.source_ref).decode().strip()
    assert hashlib.sha1(b"blob " + str(len(source)).encode() + b"\0" + source).hexdigest() == blob
    args.out.mkdir(parents=True, exist_ok=False)
    commands = []
    def run(name, argv, cwd, env=None):
        before = time.monotonic()
        result = subprocess.run([str(x) for x in argv], cwd=cwd, env=env, capture_output=True, timeout=60)
        for stream in ("stdout", "stderr"):
            (args.out / f"{name}.{stream}").write_bytes(getattr(result, stream))
        commands.append({"name": name, "argv": [str(x) for x in argv], "cwd": str(cwd),
                         "exit_code": result.returncode, "elapsed_seconds": time.monotonic() - before,
                         "stdout_sha256": digest(result.stdout), "stderr_sha256": digest(result.stderr)})
        return result
    def checked(name, argv, cwd):
        result = run(name, argv, cwd)
        assert result.returncode == 0, (name, result.stderr.decode(errors="replace"))
        return result
    helpers, variants = {}, {}
    rel = Path("crates/keld-wv/src/webview2/mod.rs")
    for name, patch in (("v2", args.red_patch), ("v3", args.patch)):
        case = args.out / name
        case.mkdir()
        checked(name + "-git-init", ["git", "init", "--quiet", "--initial-branch=proof-only"], case)
        target = case / rel
        target.parent.mkdir(parents=True)
        target.write_bytes(source)
        checked(name + "-patch-check", ["git", "-c", "core.autocrlf=false", "apply", "--check", patch], case)
        checked(name + "-patch-apply", ["git", "-c", "core.autocrlf=false", "apply", patch], case)
        text = target.read_text(encoding="utf-8")
        begin = text.index("fn report_host_initialized(")
        first = text.index("{", begin)
        depth, end = 1, first + 1
        while depth:
            if text[end] == "{": depth += 1
            elif text[end] == "}": depth -= 1
            end += 1
        helper = text[begin:end] + "\n"
        helpers[name] = helper
        (args.out / (name + "-helper.rs")).write_text(helper, encoding="utf-8")
        variants[name] = {"patch_sha256": digest(patch.read_bytes()),
                          "applied_source_sha256": digest(target.read_bytes()),
                          "helper_sha256": digest(helper.encode()),
                          "first_init_take_call_present":
                              "&& !report_host_initialized(&events, capture_desktop_at_init.take())" in text}
        assert variants[name]["first_init_take_call_present"]
        assert text.count("use crate::engine::first_host_init_turn;") == 1
    v3 = helpers["v3"]
    ordered = ("    let captured = capture_desktop_at_init.map(|capture| capture());\n"
               "    let observed_at = Instant::now();\n")
    assert v3.count(ordered) == 1, "current helper capture/timestamp shape changed"
    helpers["before_sample"] = v3.replace(
        ordered, "    let observed_at = Instant::now();\n"
                 "    let captured = capture_desktop_at_init.map(|capture| capture());\n")
    helpers["noop"] = v3[:v3.index("{") + 1] + "\n    let _ = (events, capture_desktop_at_init);\n    true\n}\n"
    harness = args.harness.read_text(encoding="utf-8")
    marker = "// W4_EXTRACTED_HELPER_MODULES"
    assert harness.count(marker) == 1
    modules = []
    for name, helper in helpers.items():
        visibility = helper.replace("fn report_host_initialized(", "pub(super) fn report_host_initialized(", 1)
        imports = "AppWindowEvent, Sender" if name == "noop" else "AppWindowEvent, Instant, Sender"
        modules.append("mod " + name + " {\nuse super::{" + imports + "};\n" + visibility + "}\n")
    generated = harness.replace(marker, "\n".join(modules))
    program = args.out / "init-seam-generated.rs"
    program.write_text(generated, encoding="utf-8")
    rustc = shutil.which("rustc")
    assert rustc, "rustc is unavailable"
    compiler = checked("rustc-version", [rustc, "--version", "--verbose"], args.out).stdout.decode()
    binary = args.out / ("init-seam-proof.exe" if os.name == "nt" else "init-seam-proof")
    checked("compile", [rustc, "--edition=2024", "--test", "-Dwarnings",
                        "--crate-name", "w4_init_seam_proof", program, "-o", binary], args.out)
    selections = [
        ("v2", "timely_capture_keeps_timestamp_across_post_enqueue_preemption", 101),
        ("v3", None, 0),
        ("before_sample", "genuinely_slow_capture_is_not_backdated", 101),
        ("noop", None, 101),
    ]
    observations = []
    for name, selection, expected in selections:
        env = os.environ.copy()
        env["W4_SEAM_VARIANT"] = name
        argv = [binary, "--test-threads=1", "--nocapture"]
        if selection: argv += ["--exact", selection]
        result = run("run-" + name, argv, args.out, env)
        text = result.stdout.decode(errors="replace")
        counts = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)
        assert counts, (name, text, result.stderr.decode(errors="replace"))
        observations.append({"variant": name, "selected_test": selection, "exit_code": result.returncode,
                             "expected_exit_code": expected, "passed": int(counts[1]),
                             "failed": int(counts[2]), "ignored": int(counts[3])})
        assert result.returncode == expected, observations[-1]
        assert (int(counts[1]), int(counts[2])) == ((8, 0) if name == "v3" else (0, 8) if name == "noop" else (0, 1))
    receipt = {"schema": "keld.w4-init-helper-proof/v1", "completed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
               "source_ref": args.source_ref, "source_blob": blob, "source_sha256": digest(source),
               "compiler": compiler, "variants": variants, "observations": observations, "commands": commands,
               "harness_sha256": digest(args.harness.read_bytes()), "driver_sha256": digest(Path(__file__).read_bytes()),
               "generated_sha256": digest(program.read_bytes()), "binary_sha256": digest(binary.read_bytes()),
               "limits": ["Exact extracted helper bodies compile in isolation with an opaque facts token.",
                          "Instant is a deterministic injected clock; no actual scheduler preemption is claimed.",
                          "Sender delegates to real std::sync::mpsc then applies the controlled post-enqueue hook.",
                          "First-init guard/Option.take call site is source checked, not an executed tao native loop.",
                          "No Windows APIs, Core generation/admission, public Ready, deadlines or native cleanup are qualified."]}
    (args.out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"receipt": str(args.out / "receipt.json"), "sha256": digest((args.out / "receipt.json").read_bytes()),
                      "observations": observations, "compiler": compiler.splitlines()[0]}))
if __name__ == "__main__":
    main()
