//! Shared `justfile` gate inventory for standalone repository contract checkers.

/// Returns the gates `recipe` runs, in declaration order, or `None` when the
/// recipe is not defined.
///
/// A dependency whose recipe has no body only groups other gates (for example a
/// `[parallel]` group), so it expands in place; every other dependency is a gate.
/// A dependency already being expanded stays a leaf, so a cycle cannot recurse.
pub(crate) fn expanded_gates<'a>(justfile: &'a str, recipe: &str) -> Option<Vec<&'a str>> {
    dependency_gates(justfile, recipe, false)
}

/// Expands declared dependencies, optionally descending through recipes with
/// bodies as well. Reachability includes the intermediate recipes themselves;
/// inventory expansion retains its existing bodyful-leaf behavior.
pub(crate) fn dependency_gates<'a>(
    justfile: &'a str,
    recipe: &str,
    include_body_dependencies: bool,
) -> Option<Vec<&'a str>> {
    let mut gates = Vec::new();
    let mut expanding = vec![recipe];
    expand(
        justfile,
        recipe_dependencies(justfile, recipe)?,
        &mut expanding,
        &mut gates,
        include_body_dependencies,
    );
    Some(gates)
}

fn expand<'a: 'b, 'b>(
    justfile: &'a str,
    dependencies: Vec<&'a str>,
    expanding: &mut Vec<&'b str>,
    gates: &mut Vec<&'a str>,
    include_body_dependencies: bool,
) {
    for dependency in dependencies {
        let group = (!expanding.contains(&dependency)
            && (include_body_dependencies || !recipe_has_body(justfile, dependency)))
        .then(|| recipe_dependencies(justfile, dependency))
        .flatten();
        match group {
            Some(members) => {
                expanding.push(dependency);
                expand(
                    justfile,
                    members,
                    expanding,
                    gates,
                    include_body_dependencies,
                );
                expanding.pop();
                if include_body_dependencies {
                    gates.push(dependency);
                }
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
    if dependencies.trim().is_empty() && matches!(recipe, "ci" | "ci-full") {
        let command = justfile
            .lines()
            .skip_while(|line| !std::ptr::eq(*line, header))
            .skip(1)
            .find(|line| !line.trim().is_empty())?
            .trim();
        let inventory = command.strip_prefix("{{python_command}} -B tools/ci_local.py ")?;
        if inventory != format!("{recipe}-inventory") {
            return None;
        }
        return recipe_dependencies(justfile, inventory);
    }
    Some(
        declaration_words(dependencies, false)
            .into_iter()
            .filter(|token| *token != "&&")
            .collect(),
    )
}

/// Splits declaration words outside quoted strings and parenthesized arguments.
/// Comments are not words. In attribute lists, brackets and commas delimit
/// words too; quoted punctuation remains part of its argument.
pub(crate) fn declaration_words(line: &str, attribute_list: bool) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, character) in line.char_indices() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && delimiter != '\'' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            continue;
        }
        if character == '#' {
            if let Some(begin) = start.take() {
                words.push(&line[begin..index]);
            }
            return words;
        }
        if depth == 0
            && (character.is_whitespace()
                || (attribute_list && matches!(character, ',' | '[' | ']')))
        {
            if let Some(begin) = start.take() {
                words.push(&line[begin..index]);
            }
            continue;
        }
        start.get_or_insert(index);
        match character {
            '\'' | '"' | '`' => quote = Some(character),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if let Some(begin) = start {
        words.push(&line[begin..]);
    }
    words
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
    use super::{declaration_words, dependency_gates, expanded_gates};

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
            expanded_gates(
                "ci: fmt-check # audit-docs atomic-protocol
",
                "ci"
            ),
            Some(vec!["fmt-check"])
        );
    }

    #[test]
    fn a_variable_assignment_is_not_a_recipe_header() {
        assert_eq!(expanded_gates("ci := \"x\"\n", "ci"), None);
    }

    #[test]
    fn declaration_words_ignore_comments_but_preserve_quoted_arguments() {
        assert_eq!(
            declaration_words(r#"[doc("a, parallel, # hash"), private] # parallel"#, true),
            vec![r#"doc("a, parallel, # hash")"#, "private"]
        );
        assert_eq!(
            declaration_words("[doc('a # hash'), parallel] \t # scheduler", true),
            vec!["doc('a # hash')", "parallel"]
        );
        assert_eq!(
            declaration_words("gate # another-gate", false),
            vec!["gate"]
        );
        assert_eq!(
            declaration_words("gate, other", false),
            vec!["gate,", "other"]
        );
    }

    #[test]
    fn reachability_descends_through_bodies_without_changing_inventory() {
        let source = "policy: helper\n\nhelper: test\n    true\n\ntest:\n    true\n";
        assert_eq!(expanded_gates(source, "policy"), Some(vec!["helper"]));
        assert_eq!(
            dependency_gates(source, "policy", true),
            Some(vec!["test", "helper"])
        );
    }

    #[test]
    fn bodyful_dependency_cycles_terminate() {
        let source = "cycle: helper\n\nhelper: cycle\n    true\n";
        assert_eq!(
            dependency_gates(source, "cycle", true),
            Some(vec!["cycle", "helper"])
        );
    }

    #[test]
    fn routed_entry_uses_its_declared_inventory_without_a_second_gate_list() {
        let source = "ci:\n    {{python_command}} -B tools/ci_local.py ci-inventory\n\nci-inventory: fmt-check clippy test\n\nfmt-check:\n    true\nclippy:\n    true\ntest:\n    true\n";
        assert_eq!(
            expanded_gates(source, "ci"),
            Some(vec!["fmt-check", "clippy", "test"])
        );
        assert_eq!(
            expanded_gates(
                &source.replace("ci_local.py ci-inventory", "ci_local.py unreviewed"),
                "ci"
            ),
            None
        );
        assert_eq!(
            expanded_gates(
                &source.replace(
                    "tools/ci_local.py ci-inventory",
                    "tools/unrelated.py ci-inventory"
                ),
                "ci"
            ),
            None
        );
    }
}
