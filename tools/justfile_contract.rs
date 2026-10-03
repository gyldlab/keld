//! Shared `justfile` gate inventory for standalone repository contract checkers.

/// Returns the gates `recipe` runs, in declaration order, or `None` when the
/// recipe is not defined.
///
/// A dependency whose recipe has no body only groups other gates (for example a
/// `[parallel]` group), so it expands in place; every other dependency is a gate.
/// A dependency already being expanded stays a leaf, so a cycle cannot recurse.
pub(crate) fn expanded_gates<'a>(justfile: &'a str, recipe: &str) -> Option<Vec<&'a str>> {
    let mut gates = Vec::new();
    let mut expanding = vec![recipe];
    expand(justfile, recipe_dependencies(justfile, recipe)?, &mut expanding, &mut gates);
    Some(gates)
}

fn expand<'a: 'b, 'b>(
    justfile: &'a str,
    dependencies: Vec<&'a str>,
    expanding: &mut Vec<&'b str>,
    gates: &mut Vec<&'a str>,
) {
    for dependency in dependencies {
        let group = (!expanding.contains(&dependency) && !recipe_has_body(justfile, dependency))
            .then(|| recipe_dependencies(justfile, dependency))
            .flatten();
        match group {
            Some(members) => {
                expanding.push(dependency);
                expand(justfile, members, expanding, gates);
                expanding.pop();
            }
            None => gates.push(dependency),
        }
    }
}

/// The dependency names after `recipe:`. `&&` only orders, so it is not a gate, and
/// a trailing `#` comment, which `just` ignores, names none.
fn recipe_dependencies<'a>(justfile: &'a str, recipe: &str) -> Option<Vec<&'a str>> {
    let header = header_line(justfile, recipe)?;
    let dependencies = &header[recipe.len() + 1..];
    let uncommented = dependencies.split_once('#').map_or(dependencies, |(code, _)| code);
    Some(
        uncommented
            .split_whitespace()
            .filter(|token| *token != "&&")
            .collect(),
    )
}

fn header_line<'a>(justfile: &'a str, recipe: &str) -> Option<&'a str> {
    justfile.lines().find(|line| {
        line.strip_prefix(recipe)
            .and_then(|rest| rest.strip_prefix(':'))
            .is_some_and(|rest| !rest.starts_with('='))
    })
}

/// A recipe has a body when the first non-blank line after its header is indented.
fn recipe_has_body(justfile: &str, recipe: &str) -> bool {
    let Some(header) = header_line(justfile, recipe) else {
        return true;
    };
    justfile
        .lines()
        .skip_while(|line| !std::ptr::eq(*line, header))
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.starts_with(' ') || line.starts_with('\t'))
}

#[cfg(test)]
mod justfile_contract_tests {
    use super::expanded_gates;

    const JUSTFILE: &str = "\
ci: policy fmt-check && deny

[parallel]
policy: atomic-protocol mermaid-ci

mermaid-ci:
    true

fmt-check:
    cargo fmt --all --check

deny:
    cargo deny check

loop: loop gate
";

    #[test]
    fn grouping_recipes_expand_in_order_and_gates_stay_leaves() {
        assert_eq!(
            expanded_gates(JUSTFILE, "ci"),
            Some(vec!["atomic-protocol", "mermaid-ci", "fmt-check", "deny"])
        );
    }

    #[test]
    fn a_flat_recipe_lists_its_own_dependencies() {
        assert_eq!(
            expanded_gates(JUSTFILE, "policy"),
            Some(vec!["atomic-protocol", "mermaid-ci"])
        );
    }

    #[test]
    fn a_missing_recipe_is_none_and_a_cycle_stays_a_leaf() {
        assert_eq!(expanded_gates(JUSTFILE, "absent"), None);
        assert_eq!(expanded_gates(JUSTFILE, "loop"), Some(vec!["loop", "gate"]));
    }

    #[test]
    fn a_trailing_comment_names_no_gate() {
        assert_eq!(
            expanded_gates("ci: fmt-check # audit-docs atomic-protocol
", "ci"),
            Some(vec!["fmt-check"])
        );
    }

    #[test]
    fn a_variable_assignment_is_not_a_recipe_header() {
        assert_eq!(expanded_gates("ci := \"x\"\n", "ci"), None);
    }
}
