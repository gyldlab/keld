//! Validates Mermaid documentation blocks without adding a workspace dependency.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const ALLOWED_TYPES: &[&str] = &[
    "flowchart",
    "sequenceDiagram",
    "stateDiagram-v2",
    "gantt",
    "erDiagram",
];

const ALLOWED_CLASS_DEFS: &[&str] = &[
    "classDef current fill:#dcfce7,stroke:#15803d,color:#052e16,stroke-width:2px",
    "classDef target fill:#dbeafe,stroke:#1d4ed8,color:#172554,stroke-width:2px",
    "classDef showcase fill:#f3e8ff,stroke:#7e22ce,color:#3b0764,stroke-width:2px,stroke-dasharray:5 3",
    "classDef gate fill:#fef3c7,stroke:#b45309,color:#451a03,stroke-width:2px",
    "classDef external fill:#e2e8f0,stroke:#475569,color:#0f172a,stroke-width:2px",
    "classDef denied fill:#fee2e2,stroke:#b91c1c,color:#450a0a,stroke-width:2px",
];

const ALLOWED_BOX_RGB: &[&str] = &[
    "220, 252, 231", // current
    "219, 234, 254", // target
    "243, 232, 255", // showcase
    "254, 243, 199", // gate
    "226, 232, 240", // external
    "254, 226, 226", // denied
];

fn docs_error(path: &Path, line: usize, detail: &str, fix: &str) -> String {
    format!(
        "KELD-DOCS006: Mermaid block at {}:{line} {detail}. {fix}",
        path.display()
    )
}

fn validate_acc_description(body: &[&str]) -> bool {
    let mut index = 0;
    while index < body.len() {
        let line = body[index].trim();
        if let Some(value) = line.strip_prefix("accDescr:") {
            return !value.trim().is_empty();
        }
        if line == "accDescr {" {
            index += 1;
            let mut has_description = false;
            while index < body.len() && body[index].trim() != "}" {
                has_description |= !body[index].trim().is_empty();
                index += 1;
            }
            return has_description && index < body.len();
        }
        index += 1;
    }
    false
}

fn validate_block(path: &Path, start_line: usize, body: &[&str]) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(first) = body
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
    else {
        errors.push(docs_error(
            path,
            start_line,
            "is empty",
            "Add one supported diagram and its accessibility metadata.",
        ));
        return errors;
    };
    let diagram_type = first.split_whitespace().next().unwrap_or_default();
    if !ALLOWED_TYPES.contains(&diagram_type) {
        errors.push(docs_error(
            path,
            start_line,
            &format!("uses diagram type `{diagram_type}` that Keld policy does not allow"),
            "Use flowchart, sequenceDiagram, stateDiagram-v2, gantt, or erDiagram as routed by AGENTS.md.",
        ));
    }

    let has_title = body.iter().any(|line| {
        line.trim()
            .strip_prefix("accTitle:")
            .is_some_and(|value| !value.trim().is_empty())
    });
    if !has_title {
        errors.push(docs_error(
            path,
            start_line,
            "has no non-empty `accTitle`",
            "Add a concise accessible title inside the Mermaid block.",
        ));
    }
    if !validate_acc_description(body) {
        errors.push(docs_error(
            path,
            start_line,
            "has no non-empty `accDescr`",
            "Add an accessible description using `accDescr:` or a non-empty `accDescr { ... }` block.",
        ));
    }

    for (offset, raw_line) in body.iter().enumerate() {
        let line = raw_line.trim().trim_end_matches(';');
        if line.contains("\\n") {
            errors.push(docs_error(
                path,
                start_line + offset + 1,
                "uses a literal `\\n` in a label",
                "Use `<br/>` inside a quoted flowchart label for renderer-stable line breaks.",
            ));
        }
        if line.starts_with("classDef ") && !ALLOWED_CLASS_DEFS.contains(&line) {
            errors.push(docs_error(
                path,
                start_line + offset + 1,
                "defines a non-canonical semantic class",
                "Reuse the exact current/target/showcase/gate/external/denied palette from AGENTS.md.",
            ));
        }
        if line.starts_with("style ")
            || line.starts_with("linkStyle ")
            || line.starts_with("%%{init:")
        {
            errors.push(docs_error(
                path,
                start_line + offset + 1,
                "uses inline or per-diagram styling outside the semantic palette",
                "Remove the override and use a canonical `classDef`; labels must carry meaning without edge/theme color.",
            ));
        }
        if let Some(rest) = line.strip_prefix("box rgb(") {
            let Some((rgb, _label)) = rest.split_once(')') else {
                errors.push(docs_error(
                    path,
                    start_line + offset + 1,
                    "has malformed `box rgb(...)` syntax",
                    "Use a complete Mermaid sequence box declaration.",
                ));
                continue;
            };
            if !ALLOWED_BOX_RGB.contains(&rgb.trim()) {
                errors.push(docs_error(
                    path,
                    start_line + offset + 1,
                    "uses a non-canonical sequence-box color",
                    "Use the RGB equivalent of a semantic palette color from AGENTS.md.",
                ));
            }
        }
    }
    errors
}

fn validate_markdown(path: &Path, contents: &str) -> Vec<String> {
    let lines: Vec<&str> = contents.lines().collect();
    let mut errors = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "```mermaid" {
            index += 1;
            continue;
        }
        let start_line = index + 1;
        index += 1;
        let body_start = index;
        while index < lines.len() && lines[index].trim() != "```" {
            index += 1;
        }
        if index == lines.len() {
            errors.push(docs_error(
                path,
                start_line,
                "has no closing code fence",
                "Close the Mermaid block with a standalone triple-backtick fence.",
            ));
            break;
        }
        errors.extend(validate_block(path, start_line, &lines[body_start..index]));
        index += 1;
    }
    errors
}

fn tracked_markdown(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--", "*.md"])
        .output()
        .map_err(|error| {
            format!(
                "KELD-DOCS005: failed to run `git ls-files` in `{}`: {error}. Install Git and pass a checkout root, then rerun `just mermaid-check`.",
                root.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "KELD-DOCS005: `git ls-files` failed in `{}` with status {}. Pass the root of a Git checkout, then rerun `just mermaid-check`.",
            root.display(),
            output.status
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        format!(
            "KELD-DOCS005: tracked Markdown path output is not UTF-8: {error}. Rename the path to UTF-8, then rerun `just mermaid-check`."
        )
    })?;
    let mut files: Vec<PathBuf> = stdout
        .split('\0')
        .filter(|relative| !relative.is_empty())
        .map(|relative| root.join(relative))
        .collect();
    files.sort();
    Ok(files)
}

fn check_files(files: Vec<PathBuf>) -> Result<usize, String> {
    let mut diagrams = 0;
    let mut errors = Vec::new();
    for path in files {
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "KELD-DOCS005: cannot inspect `{}`: {error}. Restore the tracked Markdown file, then rerun `just mermaid-check`.",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            errors.push(docs_error(
                &path,
                1,
                "is not a regular Markdown file",
                "Replace the symlink/special file with reviewed tracked Markdown.",
            ));
            continue;
        }
        let contents = fs::read_to_string(&path).map_err(|error| {
            format!(
                "KELD-DOCS005: cannot read `{}`: {error}. Restore readable UTF-8 Markdown, then rerun `just mermaid-check`.",
                path.display()
            )
        })?;
        diagrams += contents
            .lines()
            .filter(|line| line.trim() == "```mermaid")
            .count();
        errors.extend(validate_markdown(&path, &contents));
    }
    if errors.is_empty() {
        Ok(diagrams)
    } else {
        Err(errors.join("\n"))
    }
}

fn check(root: &Path) -> Result<usize, String> {
    check_files(tracked_markdown(root)?)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "KELD-DOCS005: usage: {} check <git-root> | check-file <markdown> [...]. Pass a Git root or explicit Markdown files.",
            args.first().map_or("mermaid-docs", String::as_str)
        );
        process::exit(2);
    }
    let result = match args[1].as_str() {
        "check" if args.len() == 3 => check(Path::new(&args[2])),
        "check-file" => check_files(args[2..].iter().map(PathBuf::from).collect()),
        _ => Err(
            "KELD-DOCS005: invalid arguments. Run `mermaid-docs check <git-root>` or `mermaid-docs check-file <markdown> [...]`."
                .to_owned(),
        ),
    };
    match result {
        Ok(diagrams) => println!("mermaid-docs ok: {diagrams} diagram(s) validated"),
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Path, check, validate_markdown};

    const VALID: &str = r#"# Diagram

```mermaid
flowchart LR
    accTitle: Accessible topology
    accDescr: Current input reaches a target through a policy gate.
    A["EXTERNAL input"] --> B["TARGET service"]
    classDef external fill:#e2e8f0,stroke:#475569,color:#0f172a,stroke-width:2px
    classDef target fill:#dbeafe,stroke:#1d4ed8,color:#172554,stroke-width:2px
    class A external
    class B target
```
"#;

    #[test]
    fn accessible_stable_block_passes() {
        assert!(validate_markdown(Path::new("doc.md"), VALID).is_empty());
    }

    #[test]
    fn missing_title_and_description_fail() {
        let errors = validate_markdown(
            Path::new("doc.md"),
            "```mermaid\nflowchart LR\nA --> B\n```\n",
        );
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("accTitle"));
        assert!(errors[1].contains("accDescr"));
    }

    #[test]
    fn disallowed_type_noncanonical_color_and_literal_newline_fail() {
        let errors = validate_markdown(
            Path::new("doc.md"),
            "```mermaid\narchitecture-beta\naccTitle: Bad\naccDescr: Bad syntax\nA[\\\"one\\\\ntwo\\\"]\nclassDef custom fill:#fff\n```\n",
        );
        assert_eq!(errors.len(), 3);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("policy does not allow"))
        );
        assert!(errors.iter().any(|error| error.contains("literal `\\n`")));
        assert!(errors.iter().any(|error| error.contains("non-canonical")));
    }

    #[test]
    fn unterminated_fence_fails() {
        let errors = validate_markdown(
            Path::new("doc.md"),
            "```mermaid\nflowchart LR\naccTitle: Missing fence\naccDescr: Never closes\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("no closing code fence"));
    }

    #[test]
    fn multiline_description_must_have_content_and_close() {
        let errors = validate_markdown(
            Path::new("doc.md"),
            "```mermaid\nflowchart LR\naccTitle: Empty description\naccDescr {\n}\nA --> B\n```\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("accDescr"));
    }

    #[test]
    fn inline_style_override_fails() {
        let errors = validate_markdown(
            Path::new("doc.md"),
            "```mermaid\nflowchart LR\naccTitle: Styled\naccDescr: Inline color is forbidden.\nA --> B\nstyle A fill:#fff\n```\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("outside the semantic palette"));
    }

    #[test]
    fn workspace_scan_uses_only_tracked_markdown_across_directories() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock must be after Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("keld-mermaid-docs-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("docs")).expect("create docs fixture");
        fs::create_dir_all(root.join(".agents/skills/vendor")).expect("create skill fixture");
        fs::write(root.join("README.md"), VALID).expect("write root Markdown");
        fs::write(root.join("docs/architecture.md"), VALID).expect("write docs Markdown");
        fs::write(
            root.join(".agents/skills/vendor/ignored.md"),
            "```mermaid\narchitecture-beta\n```\n",
        )
        .expect("write ignored skill Markdown");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .expect("run git init");
        assert!(init.success(), "git init must succeed");
        let add = Command::new("git")
            .args(["add", "README.md", "docs/architecture.md"])
            .current_dir(&root)
            .status()
            .expect("run git add");
        assert!(add.success(), "git add must succeed");

        let result = check(&root);
        fs::remove_dir_all(&root).expect("remove isolated fixture");

        assert_eq!(result.expect("root and docs fixtures must pass"), 2);
    }
}
