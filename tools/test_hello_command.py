"""Regression checks for the documented native hello command."""

from pathlib import Path
import shlex
import subprocess


ROOT = Path(__file__).resolve().parent.parent
OLD_COMMAND = "cargo run -p keld-host -- --hello"
EXPECTED_COMMAND = "cargo run -p keld-host --bin keld-host -- --hello"


def hello_recipe_command():
    lines = (ROOT / "justfile").read_text(encoding="utf-8").splitlines()
    start = lines.index("hello:")
    commands = []
    for line in lines[start + 1:]:
        if line and not line[0].isspace():
            break
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            commands.append(stripped)
    if len(commands) != 1:
        raise AssertionError(f"expected one command in the hello recipe, found {commands!r}")
    command = shlex.split(commands[0])
    if not command or command[-1] != "--hello":
        raise AssertionError(f"unexpected hello command shape: {commands[0]!r}")
    return command


def main():
    command = hello_recipe_command()
    probe = [*command[:-1], "--justfile-selection-probe"]
    result = subprocess.run(
        probe,
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=120,
        check=False,
    )
    expected_diagnostic = "KELD-CLI-044: unknown host argument."
    if result.returncode != 2 or expected_diagnostic not in result.stderr:
        raise AssertionError(
            "hello recipe did not reach keld-host's argument diagnostic; "
            f"exit={result.returncode}, stdout={result.stdout!r}, stderr={result.stderr!r}"
        )

    for relative in (
        "docs/onboarding/03-api-and-cli-surface.md",
        "docs/onboarding/05-development-guide.md",
    ):
        contents = (ROOT / relative).read_text(encoding="utf-8")
        if OLD_COMMAND in contents:
            raise AssertionError(f"stale ambiguous command remains in {relative}")
        if EXPECTED_COMMAND not in contents:
            raise AssertionError(f"explicit host selection is missing from {relative}")


if __name__ == "__main__":
    main()
