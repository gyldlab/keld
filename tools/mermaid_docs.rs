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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MermaidFence {
    Valid,
    Malformed,
}

fn mermaid_fence(line: &str) -> Option<MermaidFence> {
    let trimmed = line.trim();
    let marker = trimmed.get(..3)?;
    if !marker.chars().all(|ch| ch == '`' || ch == ':') {
        return None;
    }
    let rest = &trimmed[3..];
    let language = rest.trim_start();
    if language.eq_ignore_ascii_case("mermaid") {
        return Some(if marker == "```" && rest == "mermaid" {
            MermaidFence::Valid
        } else {
            MermaidFence::Malformed
        });
    }
    language
        .get(.."mermaid".len())
        .filter(|prefix| prefix.eq_ignore_ascii_case("mermaid"))
        .map(|_| MermaidFence::Malformed)
}

fn mermaid_ranges(contents: &str) -> Vec<(usize, usize, MermaidFence, bool)> {
    let lines: Vec<&str> = contents.lines().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(fence) = mermaid_fence(lines[index]) {
            let start = index;
            // A malformed colon fence must not consume the next backtick block.
            let marker = &lines[start].trim()[..3];
            index += 1;
            while index < lines.len() && lines[index].trim() != marker {
                index += 1;
            }
            let closed = index < lines.len();
            ranges.push((start, index, fence, closed));
            if closed {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    ranges
}

fn diagram_blocks(contents: &str) -> Vec<String> {
    // The pinned Mermaid CLI 11.16.0 scans raw Markdown with its own fence
    // regex; keep applicability aligned with those extracted source blocks.
    let lines: Vec<&str> = contents.lines().collect();
    mermaid_ranges(contents)
        .into_iter()
        .map(|(start, end, _, closed)| lines[start..end + usize::from(closed)].join("\n"))
        .collect()
}

fn changed_diagram_blocks(before: &str, after: &str) -> bool {
    diagram_blocks(before) != diagram_blocks(after)
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("KELD-DOCS005: cannot inspect Mermaid change inputs: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "KELD-DOCS005: cannot inspect Mermaid change inputs: git {} exited {}",
            args.join(" "),
            output.status
        ));
    }
    Ok(output.stdout)
}

fn blob_at_revision(root: &Path, revision: &str, path: &str) -> Result<String, String> {
    let spec = format!("{revision}:{path}");
    let bytes = git_output(root, &["show", &spec])?;
    String::from_utf8(bytes).map_err(|error| {
        format!("KELD-DOCS005: Mermaid source `{path}` is not UTF-8 at `{revision}`: {error}")
    })
}

fn changed_mermaid_inputs(root: &Path, base: &str, head: &str) -> Result<bool, String> {
    for revision in [base, head] {
        git_output(root, &["cat-file", "-e", &format!("{revision}^{{commit}}")])?;
    }
    let diff = git_output(
        root,
        &[
            "diff",
            "--no-renames",
            "--name-status",
            "-z",
            base,
            head,
            "--",
            "*.md",
            "docs/research",
        ],
    )?;
    let fields: Vec<&[u8]> = diff
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect();
    let mut index = 0;
    while index < fields.len() {
        let status = std::str::from_utf8(fields[index]).map_err(|error| {
            format!("KELD-DOCS005: changed Mermaid status is not UTF-8: {error}")
        })?;
        index += 1;
        let path = fields.get(index).ok_or_else(|| {
            "KELD-DOCS005: changed Mermaid path is missing from the Git diff".to_owned()
        })?;
        let path = std::str::from_utf8(path).map_err(|error| {
            format!("KELD-DOCS005: changed Markdown path is not UTF-8: {error}")
        })?;
        index += 1;

        // A changed optional nested checkout can contribute Markdown that is
        // outside this repository's tree; its full render input set is not
        // represented by ordinary root-level blob comparisons.
        if path == "docs/research" {
            return Ok(true);
        }
        if !path.ends_with(".md") {
            return Ok(true);
        }

        if status == "T" {
            return Ok(true);
        }
        let before = match status {
            "A" => None,
            "M" | "D" => Some(blob_at_revision(root, base, path)?),
            _ => return Ok(true),
        };
        let after = match status {
            "D" => None,
            "A" | "M" => Some(blob_at_revision(root, head, path)?),
            _ => return Ok(true),
        };
        if changed_diagram_blocks(
            before.as_deref().unwrap_or_default(),
            after.as_deref().unwrap_or_default(),
        ) {
            return Ok(true);
        }
    }
    Ok(false)
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
    for (start, end, fence, closed) in mermaid_ranges(contents) {
        let start_line = start + 1;
        if !closed {
            errors.push(docs_error(
                path,
                start_line,
                "has no closing code fence",
                "Close the Mermaid block with a standalone triple-backtick fence.",
            ));
            continue;
        }
        if fence == MermaidFence::Malformed {
            errors.push(docs_error(
                path,
                start_line,
                "uses a malformed Mermaid fence",
                "Use exactly ` ```mermaid` to open a Mermaid block.",
            ));
        } else {
            errors.extend(validate_block(path, start_line, &lines[start + 1..end]));
        }
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

fn untracked_markdown(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.md",
        ])
        .output()
        .map_err(|error| {
            format!(
                "KELD-DOCS005: failed to discover untracked Markdown under `{}`: {error}.",
                root.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "KELD-DOCS005: cannot discover untracked Markdown under `{}`: {}",
            root.display(),
            output.status
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        format!("KELD-DOCS005: untracked Markdown path output is not UTF-8: {error}")
    })?;
    Ok(stdout
        .split('\0')
        .filter(|relative| !relative.is_empty())
        .map(|relative| root.join(relative))
        .collect())
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
        diagrams += diagram_blocks(&contents).len();
        errors.extend(validate_markdown(&path, &contents));
    }
    if errors.is_empty() {
        Ok(diagrams)
    } else {
        Err(errors.join("\n"))
    }
}

fn working_tree_mermaid_inputs(root: &Path, base: &str) -> Result<bool, String> {
    let diff = git_output(
        root,
        &[
            "diff",
            "--no-renames",
            "--name-status",
            "-z",
            base,
            "--",
            "*.md",
            "docs/research",
        ],
    )?;
    let fields: Vec<&[u8]> = diff
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect();
    let mut index = 0;
    while index < fields.len() {
        let status = std::str::from_utf8(fields[index]).map_err(|error| {
            format!("KELD-DOCS005: changed Mermaid status is not UTF-8: {error}")
        })?;
        index += 1;
        let path = fields.get(index).ok_or_else(|| {
            "KELD-DOCS005: changed Mermaid path is missing from the Git diff".to_owned()
        })?;
        let path = std::str::from_utf8(path).map_err(|error| {
            format!("KELD-DOCS005: changed Markdown path is not UTF-8: {error}")
        })?;
        index += 1;
        if path == "docs/research" {
            return Ok(true);
        }
        if !path.ends_with(".md") {
            return Ok(true);
        }
        if status == "T" {
            return Ok(true);
        }
        let before = match status {
            "A" => None,
            "M" | "D" => Some(blob_at_revision(root, base, path)?),
            _ => return Ok(true),
        };
        let working_path = root.join(path);
        let after = match status {
            "D" => None,
            "A" | "M" => match fs::symlink_metadata(&working_path) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    Some(fs::read_to_string(&working_path).map_err(|error| {
                        format!(
                            "KELD-DOCS005: cannot read changed Mermaid source `{path}`: {error}"
                        )
                    })?)
                }
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(format!(
                        "KELD-DOCS005: cannot inspect changed Mermaid source `{path}`: {error}"
                    ));
                }
            },
            _ => return Ok(true),
        };
        if changed_diagram_blocks(
            before.as_deref().unwrap_or_default(),
            after.as_deref().unwrap_or_default(),
        ) {
            return Ok(true);
        }
    }

    let untracked = git_output(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.md",
        ],
    )?;
    for path in untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path).map_err(|error| {
            format!("KELD-DOCS005: untracked Markdown path is not UTF-8: {error}")
        })?;
        let file = root.join(path);
        let metadata = match fs::symlink_metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(true),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Ok(true);
        }
        let contents = match fs::read_to_string(&file) {
            Ok(contents) => contents,
            Err(_) => return Ok(true),
        };
        if !diagram_blocks(&contents).is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn changed_mermaid_worktree(root: &Path, base: &str) -> Result<bool, String> {
    let head = String::from_utf8(git_output(root, &["rev-parse", "HEAD"])?)
        .map_err(|error| format!("KELD-DOCS005: local HEAD is not UTF-8: {error}"))?;
    let head = head.trim();
    if changed_mermaid_inputs(root, base, head)? || working_tree_mermaid_inputs(root, head)? {
        return Ok(true);
    }

    let nested = root.join("docs/research");
    let nested_metadata = match fs::symlink_metadata(&nested) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Ok(true),
    };
    if nested_metadata.file_type().is_symlink() || !nested_metadata.is_dir() {
        return Ok(true);
    }
    let nested_top = match git_output(&nested, &["rev-parse", "--show-toplevel"]) {
        Ok(bytes) => PathBuf::from(
            String::from_utf8(bytes)
                .map_err(|error| {
                    format!("KELD-DOCS005: nested research root is not UTF-8: {error}")
                })?
                .trim(),
        ),
        Err(_) => return Ok(nested.join(".git").exists()),
    };
    let nested_top = fs::canonicalize(nested_top).map_err(|error| {
        format!("KELD-DOCS005: cannot resolve nested research Git root: {error}")
    })?;
    let canonical_nested = fs::canonicalize(&nested)
        .map_err(|error| format!("KELD-DOCS005: cannot resolve nested research root: {error}"))?;
    if nested_top != canonical_nested {
        // A plain directory under docs/research uses the main repository's
        // block comparison above; it is not a second renderer input owner.
        return Ok(false);
    }
    let upstream = match git_output(
        &nested,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    ) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|error| {
                format!("KELD-DOCS005: nested research upstream is not UTF-8: {error}")
            })?
            .trim()
            .to_owned(),
        Err(_) => return Ok(true),
    };
    let nested_head = String::from_utf8(git_output(&nested, &["rev-parse", "HEAD"])?)
        .map_err(|error| format!("KELD-DOCS005: nested research HEAD is not UTF-8: {error}"))?
        .trim()
        .to_owned();
    Ok(changed_mermaid_inputs(&nested, &upstream, &nested_head)?
        || working_tree_mermaid_inputs(&nested, &upstream)?)
}

fn check(root: &Path) -> Result<usize, String> {
    check_files(tracked_markdown(root)?)
}

fn manifest(root: &Path) -> Result<Vec<(usize, String)>, String> {
    let mut files = tracked_markdown(root)?;
    files.extend(untracked_markdown(root)?);
    files.sort();
    files.dedup();
    let mut diagrams = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "KELD-DOCS005: cannot inspect `{}`: {error}. Restore the tracked Markdown file, then rerun Mermaid validation.",
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
                "KELD-DOCS005: cannot read `{}`: {error}. Restore readable UTF-8 Markdown, then rerun Mermaid validation.",
                path.display()
            )
        })?;
        errors.extend(validate_markdown(&path, &contents));
        let count = diagram_blocks(&contents).len();
        if count > 0 {
            let relative = path.strip_prefix(root).map_err(|error| {
                format!(
                    "KELD-DOCS005: tracked Markdown `{}` escaped root `{}`: {error}",
                    path.display(),
                    root.display()
                )
            })?;
            let relative = relative.to_str().ok_or_else(|| {
                format!(
                    "KELD-DOCS005: Mermaid path `{}` is not UTF-8",
                    relative.display()
                )
            })?;
            diagrams.push((count, relative.to_owned()));
        }
    }
    if errors.is_empty() {
        Ok(diagrams)
    } else {
        Err(errors.join("\n"))
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "KELD-DOCS005: usage: {} check <git-root> | check-file <markdown> [...] | manifest <git-root> | changes <git-root> <base> <head> | worktree <git-root> <base>. Pass a Git root or explicit Markdown files.",
            args.first().map_or("mermaid-docs", String::as_str)
        );
        process::exit(2);
    }
    let result: Result<(), String> = match args[1].as_str() {
        "check" if args.len() == 3 => check(Path::new(&args[2]))
            .map(|diagrams| println!("mermaid-docs ok: {diagrams} diagram(s) validated")),
        "check-file" => check_files(args[2..].iter().map(PathBuf::from).collect())
            .map(|diagrams| println!("mermaid-docs ok: {diagrams} diagram(s) validated")),
        "manifest" if args.len() == 3 => manifest(Path::new(&args[2])).map(|diagrams| {
            let mut output = String::new();
            for (count, path) in diagrams {
                output.push_str(&count.to_string());
                output.push('\0');
                output.push_str(&path);
                output.push('\0');
            }
            print!("{output}");
        }),
        "changes" if args.len() == 5 => changed_mermaid_inputs(
            Path::new(&args[2]),
            &args[3],
            &args[4],
        )
        .map(|selected| {
            println!("{}", if selected { "true" } else { "false" });
        }),
        "worktree" if args.len() == 4 => changed_mermaid_worktree(
            Path::new(&args[2]),
            &args[3],
        )
        .map(|selected| {
            println!("{}", if selected { "true" } else { "false" });
        }),
        _ => Err(
            "KELD-DOCS005: invalid arguments. Run `mermaid-docs check <git-root>`, `manifest <git-root>`, `changes <git-root> <base> <head>`, `worktree <git-root> <base>`, or `check-file <markdown> [...]`."
                .to_owned(),
        ),
    };
    match result {
        Ok(()) => {}
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

    use super::{
        Path, PathBuf, changed_diagram_blocks, changed_mermaid_inputs, changed_mermaid_worktree,
        check, diagram_blocks, manifest, validate_markdown,
    };

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
    fn prose_only_changes_outside_mermaid_blocks_do_not_select_rendering() {
        let edited = VALID.replace("# Diagram", "# Updated diagram explanation");
        assert!(!changed_diagram_blocks(VALID, &edited));
    }

    #[test]
    fn mermaid_cli_wrappers_preserve_render_input_while_plain_prose_skips() {
        let wrapped = format!("~~~text\n{VALID}~~~\n");
        assert_eq!(diagram_blocks(VALID).len(), 1);
        assert_eq!(diagram_blocks(&wrapped).len(), 1);
        assert!(!changed_diagram_blocks(VALID, &wrapped));
        assert!(!changed_diagram_blocks(&wrapped, VALID));
        assert!(validate_markdown(Path::new("doc.md"), &wrapped).is_empty());

        for (opening, closing) in [("<!--\n", "-->\n"), ("<pre>\n", "</pre>\n")] {
            let wrapped = format!("{opening}{VALID}{closing}");
            assert_eq!(diagram_blocks(&wrapped).len(), 1);
            assert!(!changed_diagram_blocks(VALID, &wrapped));
            assert!(!changed_diagram_blocks(&wrapped, VALID));
        }
    }

    #[test]
    fn mermaid_cli_colon_fence_is_selected_as_malformed_keld_input() {
        let colon_fence = ":::mermaid\nflowchart LR\nA --> B\n:::\n";
        assert!(changed_diagram_blocks("# plain prose\n", colon_fence));
        assert!(!validate_markdown(Path::new("doc.md"), colon_fence).is_empty());

        let followed_by_valid = format!("{colon_fence}{VALID}");
        assert_eq!(diagram_blocks(&followed_by_valid).len(), 2);
        assert_eq!(validate_markdown(Path::new("doc.md"), &followed_by_valid).len(), 1);
    }

    #[test]
    fn changed_added_and_deleted_mermaid_blocks_select_rendering() {
        let changed = VALID.replace("EXTERNAL input", "EXTERNAL request");
        assert!(changed_diagram_blocks(VALID, &changed));
        assert!(changed_diagram_blocks("# no diagrams\n", VALID));
        assert!(changed_diagram_blocks(VALID, "# no diagrams\n"));
    }

    #[test]
    fn malformed_mermaid_edit_selects_validation_and_rendering() {
        let malformed = VALID.replace("```\n", "");
        assert!(changed_diagram_blocks(VALID, &malformed));
        let errors = validate_markdown(Path::new("doc.md"), &malformed);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("no closing code fence"))
        );
    }

    #[test]
    fn malformed_mermaid_fence_spelling_is_rejected() {
        let malformed = VALID.replace("```mermaid", "```mermaidx");
        let errors = validate_markdown(Path::new("doc.md"), &malformed);
        assert!(
            errors.iter().any(|error| error.contains("Mermaid fence")),
            "{errors:?}"
        );
    }

    fn git_fixture(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git fixture command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit_fixture(root: &Path, message: &str) -> String {
        git_fixture(root, &["add", "--all"]);
        git_fixture(
            root,
            &["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", message],
        );
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .expect("read fixture commit");
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .expect("fixture SHA is UTF-8")
            .trim()
            .to_owned()
    }

    fn git_change_fixture() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock must be after Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("keld-mermaid-route-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).expect("create route fixture");
        git_fixture(&root, &["init", "--quiet"]);
        // No detached `git maintenance` child may race fixture cleanup (#670).
        git_fixture(&root, &["config", "maintenance.auto", "false"]);
        git_fixture(&root, &["config", "user.name", "Mermaid route test"]);
        git_fixture(
            &root,
            &["config", "user.email", "mermaid-route@example.invalid"],
        );
        root
    }

    #[test]
    fn git_range_skips_prose_edits_but_selects_changed_added_deleted_and_renamed_blocks() {
        let root = git_change_fixture();
        fs::write(root.join("docs.md"), VALID).expect("write base diagram");
        fs::write(root.join("plain.md"), "# Plain prose\n").expect("write plain base");
        let base = commit_fixture(&root, "base");

        fs::write(
            root.join("docs.md"),
            VALID.replace("# Diagram", "# Changed prose"),
        )
        .expect("edit prose only");
        let prose_head = commit_fixture(&root, "prose only");
        assert!(!changed_mermaid_inputs(&root, &base, &prose_head).expect("classify prose edit"));

        fs::write(
            root.join("docs.md"),
            VALID.replace("EXTERNAL input", "EXTERNAL request"),
        )
        .expect("edit diagram block");
        let changed_head = commit_fixture(&root, "change diagram");
        assert!(
            changed_mermaid_inputs(&root, &prose_head, &changed_head)
                .expect("classify diagram edit")
        );

        fs::write(root.join("new.md"), VALID).expect("add diagram file");
        let added_head = commit_fixture(&root, "add diagram");
        assert!(
            changed_mermaid_inputs(&root, &changed_head, &added_head)
                .expect("classify diagram add")
        );

        git_fixture(&root, &["mv", "new.md", "renamed.md"]);
        let renamed_head = commit_fixture(&root, "rename diagram file");
        assert!(
            changed_mermaid_inputs(&root, &added_head, &renamed_head)
                .expect("classify diagram rename")
        );

        fs::remove_file(root.join("renamed.md")).expect("remove diagram file");
        let deleted_head = commit_fixture(&root, "delete diagram");
        assert!(
            changed_mermaid_inputs(&root, &renamed_head, &deleted_head)
                .expect("classify diagram delete")
        );
        assert!(changed_mermaid_inputs(&root, "missing-base", &deleted_head).is_err());
        fs::remove_dir_all(root).expect("remove route fixture");
    }

    #[test]
    fn git_range_selects_mermaid_cli_colon_fence_in_prose_document() {
        let root = git_change_fixture();
        fs::write(root.join("docs.md"), "# Plain prose\n").expect("write prose base");
        let base = commit_fixture(&root, "prose base");
        fs::write(
            root.join("docs.md"),
            "# Plain prose\n\n:::mermaid\nflowchart LR\nA --> B\n:::\n",
        )
        .expect("add Mermaid CLI colon fence");
        let head = commit_fixture(&root, "colon Mermaid fence");
        assert!(
            changed_mermaid_inputs(&root, &base, &head)
                .expect("classify colon Mermaid fence")
        );
        fs::remove_dir_all(root).expect("remove colon-fence route fixture");
    }

    #[test]
    fn manifest_lists_only_valid_markdown_files_with_diagrams() {
        let root = git_change_fixture();
        fs::write(root.join("plain.md"), "# Mermaid prose only\n").expect("write plain doc");
        git_fixture(&root, &["add", "plain.md"]);
        commit_fixture(&root, "tracked prose");
        assert!(manifest(&root).expect("plain-only manifest").is_empty());
        fs::write(root.join("diagram.md"), VALID).expect("write untracked diagram doc");
        let result = manifest(&root).expect("build diagram manifest");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, 1);
        assert_eq!(result[0].1, "diagram.md");
        fs::remove_dir_all(root).expect("remove manifest fixture");
    }

    #[test]
    fn local_route_checks_tracked_and_untracked_mermaid_content() {
        let root = git_change_fixture();
        fs::write(root.join("diagram.md"), VALID).expect("write base diagram");
        let base = commit_fixture(&root, "base");
        assert!(changed_mermaid_worktree(&root, "missing-base").is_err());

        fs::write(
            root.join("diagram.md"),
            VALID.replace("# Diagram", "# Prose only"),
        )
        .expect("edit prose outside the diagram");
        assert!(!changed_mermaid_worktree(&root, &base).expect("classify local prose edit"));

        fs::write(
            root.join("diagram.md"),
            VALID.replace("EXTERNAL input", "EXTERNAL local request"),
        )
        .expect("edit local diagram");
        assert!(changed_mermaid_worktree(&root, &base).expect("classify local diagram edit"));

        fs::write(root.join("diagram.md"), VALID).expect("restore base diagram");
        fs::write(root.join("untracked.md"), "# Plain untracked prose\n")
            .expect("write untracked prose");
        assert!(!changed_mermaid_worktree(&root, &base).expect("classify plain untracked prose"));
        fs::write(root.join("untracked.md"), VALID).expect("write untracked diagram");
        assert!(changed_mermaid_worktree(&root, &base).expect("classify untracked diagram"));
        fs::remove_file(root.join("untracked.md")).expect("remove untracked diagram");
        fs::create_dir_all(root.join("docs/research")).expect("create nested research directory");
        fs::write(root.join("docs/research/.git"), "malformed gitdir marker\n")
            .expect("write malformed nested gitdir marker");
        assert!(changed_mermaid_worktree(&root, &base).expect("fail safe for invalid nested checkout"));
        fs::remove_dir_all(root.join("docs/research")).expect("remove malformed nested checkout");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                root.join("missing-research-target"),
                root.join("docs/research"),
            )
            .expect("create dangling nested research symlink");
            assert!(
                changed_mermaid_worktree(&root, &base)
                    .expect("fail safe for dangling nested checkout")
            );
        }
        fs::remove_dir_all(root).expect("remove local route fixture");
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
        git_fixture(&root, &["init", "--quiet"]);
        // No detached `git maintenance` child may race fixture cleanup (#670).
        git_fixture(&root, &["config", "maintenance.auto", "false"]);
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
