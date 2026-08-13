//! Deterministic generator for Keld's agent-readable documentation corpus.

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const INDEX_OUTPUT: &str = "llms.txt";
const FULL_OUTPUT: &str = "llms-full.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Source {
    section: &'static str,
    title: &'static str,
    path: &'static str,
    description: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        section: "Start here",
        title: "Quickstart",
        path: "docs/onboarding/README.md",
        description: "Set up the workspace, run the implemented commands, and verify a change.",
    },
    Source {
        section: "Start here",
        title: "Wire formats and contracts",
        path: "docs/onboarding/04-wire-formats-and-contracts.md",
        description: "Byte-level kipc frames, handshake, codec, and implemented-vs-specified gaps.",
    },
    Source {
        section: "Architecture",
        title: "01 — Overview",
        path: "docs/architecture/01-overview.md",
        description: "System boundaries, design principles, crate topology, and performance budgets.",
    },
    Source {
        section: "Architecture",
        title: "02 — IPC",
        path: "docs/architecture/02-ipc.md",
        description: "kipc topology, framing, codecs, channels, and lifecycle semantics.",
    },
    Source {
        section: "Architecture",
        title: "03 — Security",
        path: "docs/architecture/03-security.md",
        description: "Principals, capabilities, default-deny enforcement, and update security.",
    },
    Source {
        section: "Architecture",
        title: "04 — Electron compatibility",
        path: "docs/architecture/04-electron-compat.md",
        description: "Migration flow, compatibility tiers, native modules, and corpus measurement.",
    },
    Source {
        section: "Architecture",
        title: "05 — Webview and native",
        path: "docs/architecture/05-webview-and-native.md",
        description: "WebEngine policy, renderer bridge, native APIs, and plugin boundaries.",
    },
    Source {
        section: "Architecture",
        title: "06 — Runtime and tooling",
        path: "docs/architecture/06-runtime-and-tooling.md",
        description: "Supervised Bun runtime, CLI, packaging, updates, and development loop.",
    },
    Source {
        section: "Architecture",
        title: "07 — Agent experience",
        path: "docs/architecture/07-agent-experience.md",
        description: "Agent-facing errors, docs, MCP tools, evals, and CLI contracts.",
    },
    Source {
        section: "Engineering",
        title: "Decision log",
        path: "docs/engineering/decisions.md",
        description: "What we chose, why, what we rejected, and what is not next.",
    },
    Source {
        section: "Reference",
        title: "KELD error registry",
        path: "docs/engineering/keld-error-codes.md",
        description: "Stable developer-facing error codes with messages and fixes.",
    },
    Source {
        section: "Reference",
        title: "Size, RSS, and installer scoreboard",
        path: "docs/engineering/budget-scoreboard.md",
        description: "Living hello-to-apps measurements against architecture 01 §5 budgets and competitors.",
    },
    Source {
        section: "Reference",
        title: "Electron compatibility scoreboard",
        path: "docs/engineering/compat-scoreboard.md",
        description: "Current measurement status and the future public scoreboard contract.",
    },
];

#[derive(Debug)]
struct LoadedSource {
    source: Source,
    contents: String,
}

fn path_escapes_corpus(candidate: &Path) -> bool {
    let forbidden = [
        Path::new("docs/research"),
        Path::new("competitors"),
        Path::new(".claude"),
        Path::new("private"),
        Path::new(".private"),
    ];
    candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || forbidden.iter().any(|prefix| candidate.starts_with(prefix))
}

fn docs003(path: &str) -> String {
    format!(
        "KELD-DOCS003: source path `{path}` is outside the authoritative docs corpus. \
         Remove it from the source list; include only reviewed public documentation."
    )
}

fn validate_source_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if path_escapes_corpus(candidate) {
        return Err(docs003(path));
    }
    if candidate
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("md")
    {
        return Err(format!(
            "KELD-DOCS003: source path `{path}` is not Markdown. \
             Replace it with a reviewed `.md` source."
        ));
    }
    Ok(())
}

fn normalize_newlines(contents: &str) -> String {
    let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
    let mut normalized = normalized.trim_end_matches('\n').to_owned();
    normalized.push('\n');
    normalized
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(root).map_err(|error| {
        format!(
            "KELD-DOCS005: failed to resolve workspace root `{}`: {error}. \
             Pass an existing directory, then rerun the generator.",
            root.display()
        )
    })
}

fn resolve_source_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_source_path(relative)?;
    let joined = root.join(relative);
    let canonical = match fs::canonicalize(&joined) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(format!(
                "KELD-DOCS001: required docs source `{relative}` is missing. \
                 Restore the file or remove its entry from `tools/llms_docs.rs`."
            ));
        }
        Err(error) => {
            return Err(format!(
                "KELD-DOCS005: failed to read docs source `{relative}`: {error}. \
                 Check that the file is readable, then rerun the generator."
            ));
        }
    };
    let Ok(stripped) = canonical.strip_prefix(root) else {
        return Err(docs003(relative));
    };
    // Re-check after canonicalize so a symlink cannot land in a forbidden
    // directory that is still inside the workspace (e.g. docs/research).
    if path_escapes_corpus(stripped) {
        return Err(docs003(relative));
    }
    Ok(canonical)
}

fn load_sources(root: &Path, sources: &[Source]) -> Result<Vec<LoadedSource>, String> {
    let root = canonical_root(root)?;
    let mut loaded = Vec::with_capacity(sources.len());
    for &source in sources {
        let path = resolve_source_file(&root, source.path)?;
        let contents = fs::read_to_string(&path).map_err(|error| {
            format!(
                "KELD-DOCS005: failed to read docs source `{}`: {error}. \
                 Check that the file is readable, then rerun the generator.",
                source.path
            )
        })?;
        if contents.trim().is_empty() {
            return Err(format!(
                "KELD-DOCS002: required docs source `{}` is empty. \
                 Add authoritative Markdown content or remove its source-list entry.",
                source.path
            ));
        }
        loaded.push(LoadedSource {
            source,
            contents: normalize_newlines(&contents),
        });
    }
    Ok(loaded)
}

fn render_index(loaded: &[LoadedSource]) -> String {
    let mut output = String::from(
        "# Keld\n\n\
         > Keld is a desktop framework with a Rust host, supervised Bun app process, \
         system webviews, typed kipc, and host-enforced default-deny permissions.\n\n\
         This file is generated from tracked authoritative Markdown by \
         `tools/llms_docs.rs`; do not edit it by hand. The full ordered corpus is in \
         [llms-full.txt](llms-full.txt).\n",
    );
    let mut current_section = "";
    for item in loaded {
        if item.source.section != current_section {
            output.push_str("\n## ");
            output.push_str(item.source.section);
            output.push('\n');
            current_section = item.source.section;
        }
        output.push_str("\n- [");
        output.push_str(item.source.title);
        output.push_str("](");
        output.push_str(item.source.path);
        output.push_str("): ");
        output.push_str(item.source.description);
        output.push('\n');
    }
    output.push_str(
        "\n## Corpus policy\n\n\
         `llms-full.txt` contains exactly the sources listed above, in this order. \
         Exploratory research, local-only material, generated outputs, and every \
         unlisted document are excluded.\n",
    );
    output
}

fn render_full(loaded: &[LoadedSource]) -> String {
    let mut output = String::from(
        "# Keld full documentation corpus\n\n\
         > Generated by `tools/llms_docs.rs` from tracked authoritative Markdown. \
         Do not edit this file by hand. See [llms.txt](llms.txt) for the compact index.\n",
    );
    for item in loaded {
        output.push_str("\n---\n\n## Source: `");
        output.push_str(item.source.path);
        output.push_str("`\n\n");
        output.push_str(&item.contents);
    }
    output
}

fn render(root: &Path, sources: &[Source]) -> Result<(String, String), String> {
    let loaded = load_sources(root, sources)?;
    Ok((render_index(&loaded), render_full(&loaded)))
}

fn write_output(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| {
        format!(
            "KELD-DOCS005: failed to write generated output `{}`: {error}. \
             Check that the workspace is writable, then rerun the generator.",
            path.display()
        )
    })
}

fn generate(root: &Path, sources: &[Source]) -> Result<(), String> {
    let root = canonical_root(root)?;
    let (index, full) = render(&root, sources)?;
    write_output(&root.join(INDEX_OUTPUT), &index)?;
    write_output(&root.join(FULL_OUTPUT), &full)?;
    Ok(())
}

fn check_one(path: &Path, expected: &str) -> Result<(), String> {
    let actual = fs::read(path).map_err(|error| {
        format!(
            "KELD-DOCS004: generated output `{}` is missing or unreadable: {error}. \
             Run `just llms` (or the documented rustc generator command) and commit the result.",
            path.display()
        )
    })?;
    if actual == expected.as_bytes() {
        return Ok(());
    }
    Err(format!(
        "KELD-DOCS004: generated output `{}` is stale. \
         Run `just llms` (or the documented rustc generator command) and commit the result.",
        path.display()
    ))
}

fn check(root: &Path, sources: &[Source]) -> Result<(), String> {
    let root = canonical_root(root)?;
    let (index, full) = render(&root, sources)?;
    check_one(&root.join(INDEX_OUTPUT), &index)?;
    check_one(&root.join(FULL_OUTPUT), &full)?;
    Ok(())
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "KELD-DOCS005: missing command. Run `llms-docs generate [workspace]` or \
         `llms-docs check [workspace]`."
            .to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "KELD-DOCS005: too many arguments. Run `llms-docs generate [workspace]` or \
             `llms-docs check [workspace]`."
                .to_owned(),
        );
    }
    match command.as_str() {
        "generate" => generate(&root, SOURCES),
        "check" => check(&root, SOURCES),
        _ => Err(format!(
            "KELD-DOCS005: unknown command `{command}`. \
             Use `generate` to write outputs or `check` to verify them."
        )),
    }
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    const FIXTURE_SOURCES: &[Source] = &[
        Source {
            section: "Start",
            title: "First",
            path: "docs/onboarding/first.md",
            description: "First fixture.",
        },
        Source {
            section: "Reference",
            title: "Second",
            path: "docs/engineering/second.md",
            description: "Second fixture.",
        },
    ];

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("keld-llms-docs-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated fixture root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create fixture parent");
            fs::write(path, contents).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(FIXTURE_SOURCES[0].path, "# First\r\n\r\nAlpha\r\n");
        temp.write(FIXTURE_SOURCES[1].path, "# Second\n\nBeta\n");
        temp
    }

    #[test]
    fn deterministic_rerun_is_byte_identical_and_normalizes_newlines() {
        let temp = fixture();
        generate(temp.path(), FIXTURE_SOURCES).expect("first generation");
        let first_index = fs::read(temp.path().join(INDEX_OUTPUT)).expect("read first index");
        let first_full = fs::read(temp.path().join(FULL_OUTPUT)).expect("read first corpus");

        generate(temp.path(), FIXTURE_SOURCES).expect("second generation");
        let second_index = fs::read(temp.path().join(INDEX_OUTPUT)).expect("read second index");
        let second_full = fs::read(temp.path().join(FULL_OUTPUT)).expect("read second corpus");

        assert_eq!(first_index, second_index);
        assert_eq!(first_full, second_full);
        assert!(!second_full.contains(&b'\r'));
    }

    #[test]
    fn source_headings_follow_declared_order() {
        let temp = fixture();
        let (index, full) = render(temp.path(), FIXTURE_SOURCES).expect("render fixtures");

        let first_heading = full
            .find("## Source: `docs/onboarding/first.md`")
            .expect("first source heading");
        let second_heading = full
            .find("## Source: `docs/engineering/second.md`")
            .expect("second source heading");
        assert!(first_heading < second_heading);

        let first_link = index
            .find("](docs/onboarding/first.md)")
            .expect("first index link");
        let second_link = index
            .find("](docs/engineering/second.md)")
            .expect("second index link");
        assert!(first_link < second_link);
    }

    #[test]
    fn stale_check_fails_after_source_change() {
        let temp = fixture();
        generate(temp.path(), FIXTURE_SOURCES).expect("generate fixtures");
        check(temp.path(), FIXTURE_SOURCES).expect("fresh outputs pass");

        temp.write(FIXTURE_SOURCES[0].path, "# First\n\nChanged contract.\n");
        let error = check(temp.path(), FIXTURE_SOURCES).expect_err("stale output must fail");
        assert!(error.contains("KELD-DOCS004"), "{error}");
        assert!(error.contains("just llms"), "{error}");
    }

    #[test]
    fn unlisted_and_forbidden_sources_cannot_leak() {
        let temp = fixture();
        temp.write("docs/research/secret.md", "RESEARCH_SENTINEL");
        temp.write("competitors/private.md", "COMPETITOR_SENTINEL");
        temp.write(".claude/private.md", "PRIVATE_SENTINEL");

        let (index, full) = render(temp.path(), FIXTURE_SOURCES).expect("render allowlist");
        for sentinel in [
            "RESEARCH_SENTINEL",
            "COMPETITOR_SENTINEL",
            "PRIVATE_SENTINEL",
        ] {
            assert!(!index.contains(sentinel), "{sentinel} leaked into index");
            assert!(!full.contains(sentinel), "{sentinel} leaked into corpus");
        }

        let forbidden = [Source {
            section: "Bad",
            title: "Research",
            path: "docs/research/secret.md",
            description: "Must be rejected.",
        }];
        let error = render(temp.path(), &forbidden).expect_err("forbidden path must fail");
        assert!(error.contains("KELD-DOCS003"), "{error}");
        assert!(error.contains("authoritative docs corpus"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_outside_root_is_docs003() {
        let temp = fixture();
        let outside = TempDir::new();
        outside.write("secret.md", "OUTSIDE_SENTINEL\n");
        let link = temp.path().join(FIXTURE_SOURCES[0].path);
        fs::remove_file(&link).expect("remove fixture file before replacing with symlink");
        std::os::unix::fs::symlink(outside.path().join("secret.md"), &link).expect("symlink");

        let error = render(temp.path(), FIXTURE_SOURCES)
            .expect_err("symlink pointing outside the root must fail");
        assert!(error.contains("KELD-DOCS003"), "{error}");
        assert!(error.contains(FIXTURE_SOURCES[0].path), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_into_forbidden_docs_dir_is_docs003() {
        let temp = fixture();
        temp.write("docs/research/secret.md", "RESEARCH_SENTINEL\n");
        let link = temp.path().join(FIXTURE_SOURCES[0].path);
        fs::remove_file(&link).expect("remove fixture file before replacing with symlink");
        std::os::unix::fs::symlink(temp.path().join("docs/research/secret.md"), &link)
            .expect("symlink into forbidden dir");

        let error = render(temp.path(), FIXTURE_SOURCES)
            .expect_err("symlink into docs/research must fail even though the allowlist path looks safe");
        assert!(error.contains("KELD-DOCS003"), "{error}");
        assert!(error.contains(FIXTURE_SOURCES[0].path), "{error}");
    }

    #[test]
    fn missing_and_empty_sources_report_actionable_errors() {
        let temp = TempDir::new();
        let missing = render(temp.path(), FIXTURE_SOURCES).expect_err("missing source must fail");
        assert!(missing.contains("KELD-DOCS001"), "{missing}");
        assert!(missing.contains("Restore"), "{missing}");

        temp.write(FIXTURE_SOURCES[0].path, " \r\n");
        temp.write(FIXTURE_SOURCES[1].path, "# Second\n");
        let empty = render(temp.path(), FIXTURE_SOURCES).expect_err("empty source must fail");
        assert!(empty.contains("KELD-DOCS002"), "{empty}");
        assert!(empty.contains("Add authoritative Markdown"), "{empty}");
    }
}
