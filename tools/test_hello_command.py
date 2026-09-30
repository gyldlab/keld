"""Regression checks for host selection in the native hello command."""

from pathlib import Path
import shlex


ROOT = Path(__file__).resolve().parent.parent
OLD_COMMAND = "cargo run -p keld-host -- --hello"
EXPECTED_COMMAND = [
    "cargo",
    "run",
    "-p",
    "keld-host",
    "--bin",
    "keld-host",
    "--",
    "--hello",
]
DOCUMENTED_COMMAND = "cargo run -p keld-host --bin keld-host -- --hello"


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
    return command


def main():
    command = hello_recipe_command()
    if command != EXPECTED_COMMAND:
        raise AssertionError(
            "hello recipe must select the host binary explicitly; "
            f"found {shlex.join(command)!r}"
        )

    for relative in (
        "docs/onboarding/03-api-and-cli-surface.md",
        "docs/onboarding/05-development-guide.md",
    ):
        contents = (ROOT / relative).read_text(encoding="utf-8")
        if OLD_COMMAND in contents:
            raise AssertionError(f"stale ambiguous command remains in {relative}")
        if DOCUMENTED_COMMAND not in contents:
            raise AssertionError(f"explicit host selection is missing from {relative}")


if __name__ == "__main__":
    main()
