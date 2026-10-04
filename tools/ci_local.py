"""Execute the justfile's inventory using the shared router's local selection.

The justfile owns commands, inventory and scheduling. This executor owns only
selection admission and child status propagation; direct recipes remain full checks.
"""

import concurrent.futures
import json
from pathlib import Path
import subprocess
import sys


def inventory(root: Path) -> dict:
    return json.loads(subprocess.check_output(
        ["just", "--justfile", str(root / "justfile"), "--dump", "--dump-format", "json"],
        cwd=root, text=True, encoding="utf-8"))["recipes"]


def leaves(recipes: dict, name: str, visiting=(), *, through_bodies=False) -> list[str]:
    if name in visiting:
        raise ValueError(f"cyclic CI inventory: {name}")
    recipe = recipes[name]
    if recipe["parameters"] or any(item["arguments"] for item in recipe["dependencies"]):
        raise ValueError(f"parameterized CI inventory is unsupported: {name}")
    if recipe["body"] and not through_bodies:
        return [name]
    result = []
    for dependency in recipe["dependencies"]:
        result.extend(leaves(recipes, dependency["recipe"], (*visiting, name), through_bodies=through_bodies))
    if recipe["body"]:
        result.append(name)
    return result


def selection(output: str, gates: list[str]) -> dict[str, bool]:
    values = {}
    for line in output.splitlines():
        key, sep, value = line.partition("=")
        if not sep or key in values:
            raise ValueError("malformed or duplicate router output")
        values[key] = value
    if values.get("local_contract") != "v1":
        raise ValueError("missing local input contract")
    if values.get("local_default") != "true":
        raise ValueError("unknown gates must default to selected")
    selected = {}
    for gate in gates:
        value = values.get("local_" + gate, "true")
        if value not in ("true", "false"):
            raise ValueError(f"missing or invalid applicability: {gate}")
        selected[gate] = value == "true"
    return selected


def validate_inventory(recipes: dict, name: str) -> list[str]:
    gates = leaves(recipes, name)
    if len(gates) != len(set(gates)):
        raise ValueError("duplicate CI gate in justfile inventory")
    # --no-deps is safe only when every public recipe prerequisite is separately
    # present in this inventory. It permits skipping self-tests while retaining
    # a live check, without changing what direct `just <recipe>` means.
    for gate in gates:
        for dependency in recipes[gate]["dependencies"]:
            for child in leaves(recipes, dependency["recipe"]):
                if child not in gates:
                    raise ValueError(f"unlisted prerequisite {child} of {gate}")
    suffix = ["fmt-check", "clippy", "test", "doc", "deny"]
    if gates[-len(suffix):] != suffix:
        raise ValueError("the serial Rust suffix must follow every policy gate")
    for group, recipe in recipes.items():
        if ("parallel" in recipe["attributes"]
                and set(leaves(recipes, group, through_bodies=True)) & {"typescript", *suffix}):
            raise ValueError(f"parallel recipe reaches a TypeScript or Rust gate: {group}")
    return gates


def execute(root: Path, recipes: dict, name: str, selected: dict[str, bool]) -> int:
    validate_inventory(recipes, name)

    def run(recipe_name: str) -> int:
        recipe = recipes[recipe_name]
        if recipe["body"]:
            if not selected[recipe_name]:
                return 0
            return subprocess.run(["just", "--no-deps", recipe_name], cwd=root).returncode
        children = [item["recipe"] for item in recipe["dependencies"]]
        if "parallel" in recipe["attributes"]:
            # Drain every started child before returning a failure. A policy
            # failure never starts the serial Rust suffix.
            with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, len(children))) as pool:
                results = list(pool.map(run, children))
            return next((code for code in results if code), 0)
        for child in children:
            code = run(child)
            if code:
                return code
        return 0

    return run(name)


def main() -> int:
    root = Path.cwd()
    name = sys.argv[1]
    recipes = inventory(root)
    gates = validate_inventory(recipes, name)
    output = subprocess.check_output(
        ["just", "--no-deps", "ci-route"], cwd=root, text=True, encoding="utf-8")
    selected = selection(output, gates)
    if name == "ci-full-inventory":
        for gate in leaves(recipes, "mermaid-full"):
            selected[gate] = True
    print("Local CI selected: " + " ".join(g for g in gates if selected[g]), flush=True)
    print("Local CI omitted: " + " ".join(g for g in gates if not selected[g]), flush=True)

    return execute(root, recipes, name, selected)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"local CI: {error}; refusing skipped-green execution", file=sys.stderr)
        sys.exit(1)
