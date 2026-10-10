//! GH-508 channel table: no hand-written channel id (criterion 6) and no
//! capability-name literal outside `keld-guard` (criterion 15) in production Rust.
//!
//! Spec: `docs/specs/gh508-kipc-channel-table.md` §3. The oracle is the spec's
//! literal rule set (scan input, test-only exclusions, two patterns), applied to
//! the real tree; each named negative control mutates one real source in memory.
//!
//! It lives in `keld-cli` beside the `KELD-*` registry scan because both read
//! every crate's sources: `tools/ci-inputs.json` routes any `crates/*` change to
//! this package (the workspace external-reads edge), so a literal reintroduced in
//! any crate is scanned on the PR that adds it.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "support/rust_source_scan.rs"]
mod scan;

use scan::{Hit, Sources};

const TABLE: &str = "crates/keld-ipc/src/channel_table.rs";
const GUARD_SRC: &str = "crates/keld-guard/src";

fn repo_root() -> PathBuf {
    scan::normalize(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn real_sources() -> Sources {
    scan::load_scan_inputs(&repo_root())
}

fn channel_hits(sources: &Sources) -> Vec<Hit> {
    scan::channel_literal_hits(sources, Path::new(TABLE))
}

fn hits_in<'a>(hits: &'a [Hit], file: &str) -> Vec<&'a Hit> {
    hits.iter().filter(|hit| hit.file == file).collect()
}

fn source<'a>(sources: &'a Sources, file: &str) -> &'a str {
    sources
        .get(Path::new(file))
        .unwrap_or_else(|| panic!("scan input {file} is missing"))
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "fixture anchor `{from}` must occur exactly once"
    );
    text.replacen(from, to, 1)
}

fn with_file(mut sources: Sources, file: &str, text: String) -> Sources {
    sources.insert(PathBuf::from(file), text);
    sources
}

fn synthetic(files: &[(&str, &str)]) -> Sources {
    files
        .iter()
        .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
        .collect()
}

#[test]
fn production_rust_has_no_hand_written_channel_id_outside_the_table() {
    let hits = channel_hits(&real_sources());
    assert!(
        hits.is_empty(),
        "hand-written kipc channel ids outside {TABLE} (spec gh508 criterion 6); \
         derive them from keld_ipc::channel_table instead: {hits:#?}"
    );
}

/// Prerequisite: the scan input is the spec's file set, not an empty or
/// mis-rooted walk that would make the zero-hit oracle vacuous.
#[test]
fn scan_input_is_crate_sources_and_fuzz_targets_never_tests_directories() {
    let sources = real_sources();
    for required in [
        "crates/keld-ipc/src/echo.rs",
        "crates/keld-ipc/src/lifecycle.rs",
        "crates/keld-ipc/src/receive.rs",
        "crates/keld-ipc/src/link.rs",
        "crates/keld-native/src/fs.rs",
        "crates/keld-wv/src/wkwebview/macos_bridge.rs",
        "crates/keld-ipc/fuzz/fuzz_targets/raw_receive.rs",
        TABLE,
    ] {
        assert!(
            sources.contains_key(Path::new(required)),
            "{required} must be scanned"
        );
    }
    let reached = scan::included_files(&sources);
    for path in sources.keys() {
        let spec_input = path.starts_with("crates/keld-ipc/fuzz/fuzz_targets")
            || (path
                .components()
                .nth(2)
                .is_some_and(|c| c.as_os_str() == "src")
                && !path.components().any(|c| c.as_os_str() == "tests"));
        assert!(
            spec_input || reached.contains(path),
            "{} is neither a crates/*/src or fuzz-target input nor reached by production code",
            path.display()
        );
    }
}

#[test]
fn only_directly_test_gated_items_are_excluded() {
    let excluded = [
        "#[cfg(test)]\nmod tests {\n    const X: ChannelId = ChannelId(9);\n}\n",
        "#[cfg(all(test, windows))]\nmod named_pipe_tests {\n    const X: ChannelId = ChannelId(9);\n}\n",
        "#[cfg(all(windows, test))]\n#[allow(unsafe_code)]\n// why\nmod t {\n    fn f() { let _ = ChannelId(9); }\n}\n",
        "#[cfg(test)]\nuse crate::frame::ChannelId as _Unused; // ChannelId(9)\n",
        "#[cfg(test)]\nmod tests {\n    const BRACE: &str = \"}\";\n    const C: char = '}';\n    /* } */\n    const X: ChannelId = ChannelId(9);\n}\n",
    ];
    for text in excluded {
        let hits = channel_hits(&synthetic(&[("crates/x/src/lib.rs", text)]));
        assert!(
            hits.is_empty(),
            "test-gated item must be excluded:\n{text}\n{hits:#?}"
        );
    }
    let scanned = [
        (
            "#[cfg(any(test, windows))]\nmod t {\n    const X: ChannelId = ChannelId(9);\n}\n",
            3,
        ),
        (
            "#[cfg(all(any(test, unix), windows))]\nmod t {\n    const X: ChannelId = ChannelId(9);\n}\n",
            3,
        ),
        (
            "#[cfg(not(test))]\nmod t {\n    const X: ChannelId = ChannelId(9);\n}\n",
            3,
        ),
        (
            "#[cfg(windows)]\nmod t {\n    const X: ChannelId = ChannelId(9);\n}\n",
            3,
        ),
        ("#![cfg(test)]\nconst X: ChannelId = ChannelId(9);\n", 2),
        // A multi-line non-module item is not one of the excluded forms.
        (
            "#[cfg(test)]\nstd::thread_local! {\n    static X: ChannelId = ChannelId(2);\n}\n",
            3,
        ),
        // The exclusion is the gated line only, never the rest of the file.
        (
            "#[cfg(test)]\nuse std::cell::RefCell;\n\nfn write_hello() {\n    write(ChannelId(0));\n}\n",
            5,
        ),
        // A brace inside a string must not close the test module early.
        (
            "#[cfg(test)]\nmod t {\n    const S: &str = \"{\";\n}\nconst X: ChannelId = ChannelId(4);\n",
            5,
        ),
        // Comments are part of the remaining text.
        ("// ChannelId(3) in prose\n", 1),
        ("const X: ChannelId = ChannelId(\n    2,\n);\n", 1),
        ("pub const RENDERER_CHANNEL: u16 = 1;\n", 1),
        ("const ECHO_CHANNEL : u16\n    = 1;\n", 1),
    ];
    for (text, line) in scanned {
        let hits = channel_hits(&synthetic(&[("crates/x/src/lib.rs", text)]));
        assert_eq!(
            hits,
            vec![Hit {
                file: "crates/x/src/lib.rs".to_owned(),
                line
            }],
            "must be scanned and hit:\n{text}"
        );
    }
    for text in [
        "const X: ChannelId = ChannelId(id);\n",
        "const ECHO_CHANNEL: u16 = ECHO.id().0;\n",
        "const ECHO_CHANNEL: ChannelId = ECHO.id();\n",
        "const echo_channel: u16 = 1;\n",
        "let channel = header.channel.0 == 1;\n",
    ] {
        assert!(
            channel_hits(&synthetic(&[("crates/x/src/lib.rs", text)])).is_empty(),
            "{text}"
        );
    }
}

#[test]
fn out_of_line_test_modules_resolve_by_rust_module_rules() {
    let literal = "const X: ChannelId = ChannelId(1);\n";
    let sources = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "#[cfg(test)]\nmod a;\n#[cfg(test)]\n#[path = \"support/b_tests.rs\"]\nmod b;\nmod bootstrap;\n",
        ),
        ("crates/x/src/a.rs", literal),
        ("crates/x/src/support/b_tests.rs", literal),
        (
            "crates/x/src/bootstrap.rs",
            "#[cfg(test)]\nmod deadline_tests;\nmod shape;\n",
        ),
        ("crates/x/src/bootstrap/deadline_tests.rs", literal),
        ("crates/x/src/bootstrap/shape.rs", literal),
        ("crates/x/src/deadline_tests.rs", literal),
        ("crates/x/src/inner/mod.rs", "#[cfg(test)]\nmod t;\n"),
        ("crates/x/src/inner/t/mod.rs", literal),
    ]);
    let files: BTreeSet<String> = channel_hits(&sources)
        .into_iter()
        .map(|hit| hit.file)
        .collect();
    assert_eq!(
        files,
        BTreeSet::from([
            // Not gated: `mod shape;` carries no test attribute.
            "crates/x/src/bootstrap/shape.rs".to_owned(),
            // `bootstrap.rs` resolves `deadline_tests` under `bootstrap/`, not beside it.
            "crates/x/src/deadline_tests.rs".to_owned(),
        ])
    );
}

#[test]
fn named_criterion_six_controls_fail_on_real_sources() {
    let sources = real_sources();

    let bridge = "crates/keld-wv/src/wkwebview/macos_bridge.rs";
    let mutated = format!(
        "{}\nconst ECHO_CHANNEL: u16 = 1;\n",
        source(&sources, bridge)
    );
    assert_eq!(
        hits_in(
            &channel_hits(&with_file(sources.clone(), bridge, mutated)),
            bridge
        )
        .len(),
        1
    );

    let link = "crates/keld-ipc/src/link.rs";
    let link_text = source(&sources, link);
    assert!(
        link_text.starts_with("//! Framed read/write")
            && link_text.contains("\n#[cfg(test)]\nuse std::cell::RefCell;\n"),
        "prerequisite: link.rs still gates a single `use` line with #[cfg(test)]"
    );
    let write_hello = link_text
        .find("fn write_hello")
        .expect("write_hello exists");
    let hello_tail = &link_text[write_hello..];
    let anchor = hello_tail
        .find("HANDSHAKE_CHANNEL")
        .expect("write_hello names HANDSHAKE_CHANNEL");
    let mutated = format!(
        "{}ChannelId(0){}",
        &link_text[..write_hello + anchor],
        &hello_tail[anchor + "HANDSHAKE_CHANNEL".len()..]
    );
    assert_eq!(
        hits_in(
            &channel_hits(&with_file(sources.clone(), link, mutated)),
            link
        )
        .len(),
        1
    );

    let fs = "crates/keld-native/src/fs.rs";
    let fs_text = source(&sources, fs);
    let gate = fs_text
        .find("#[cfg(test)]\nstd::thread_local! {\n")
        .expect("fs.rs test-only thread_local");
    let close = gate + fs_text[gate..].find("\n}\n").expect("thread_local closes") + "\n}\n".len();
    let mutated = format!(
        "{}const PROBE: ChannelId = ChannelId(2);\n{}",
        &fs_text[..close],
        &fs_text[close..]
    );
    assert_eq!(
        hits_in(&channel_hits(&with_file(sources.clone(), fs, mutated)), fs).len(),
        1
    );

    let echo = "crates/keld-ipc/src/echo.rs";
    let gated = format!(
        "{}\n#[cfg(test)]\nmod probe {{\n    const P: crate::ChannelId = crate::ChannelId(9);\n}}\n",
        source(&sources, echo)
    );
    assert!(
        hits_in(
            &channel_hits(&with_file(sources.clone(), echo, gated.clone())),
            echo
        )
        .is_empty()
    );
    let any = replace_once(
        &gated,
        "#[cfg(test)]\nmod probe",
        "#[cfg(any(test, windows))]\nmod probe",
    );
    assert_eq!(
        hits_in(&channel_hits(&with_file(sources.clone(), echo, any)), echo).len(),
        1
    );
}

/// The live test-only literals the spec names stay excluded, and they are real
/// pattern matches (so the exclusion, not their absence, is what passes).
#[test]
fn live_test_only_literals_are_real_matches_and_stay_excluded() {
    let sources = real_sources();
    let hits = channel_hits(&sources);
    for file in [
        "crates/keld-ipc/src/bootstrap.rs",
        "crates/keld-ipc/src/bootstrap/admission_deadline_tests.rs",
    ] {
        assert!(
            !scan::channel_literal_lines(source(&sources, file)).is_empty(),
            "prerequisite: {file} carries a test-only channel literal"
        );
        assert!(hits_in(&hits, file).is_empty(), "{file}: {hits:#?}");
    }
    let bootstrap = source(&sources, "crates/keld-ipc/src/bootstrap.rs");
    assert!(bootstrap.contains("#[cfg(all(test, windows))]\nmod named_pipe_tests {"));
    assert!(bootstrap.contains("#[cfg(test)]\nmod admission_deadline_tests;"));
}

fn exported_capability_names() -> BTreeSet<String> {
    let lib = std::fs::read_to_string(repo_root().join(GUARD_SRC).join("lib.rs"))
        .expect("read keld-guard lib.rs");
    scan::exported_capability_names(&lib)
}

fn capability_hits(sources: &Sources) -> Vec<Hit> {
    scan::capability_literal_hits(sources, Path::new(GUARD_SRC), &exported_capability_names())
}

/// Prerequisite: the scanned name set is read from `keld_guard::capability`
/// and contains the real exported constants (so it cannot silently be empty).
#[test]
fn scanned_capability_names_are_read_from_keld_guard_exports() {
    let names = exported_capability_names();
    for exported in [
        keld_guard::capability::FS_READ,
        keld_guard::capability::FS_WRITE,
    ] {
        assert!(
            names.contains(exported),
            "{exported} missing from {names:?}"
        );
    }
}

#[test]
fn production_rust_has_no_capability_literal_outside_keld_guard() {
    let hits = capability_hits(&real_sources());
    assert!(
        hits.is_empty(),
        "capability-name string literals outside {GUARD_SRC} (spec gh508 criterion 15); \
         reference keld_guard::capability constants instead: {hits:#?}"
    );
}

#[test]
fn criterion_fifteen_controls_fail_on_real_sources() {
    let sources = real_sources();

    let table = source(&sources, TABLE);
    let mutated = replace_once(
        table,
        "Authority::Guarded(&[FS_READ, FS_WRITE])",
        "Authority::Guarded(&[\"fs.read\"])",
    );
    assert_eq!(
        hits_in(
            &capability_hits(&with_file(sources.clone(), TABLE, mutated)),
            TABLE
        )
        .len(),
        1
    );

    let dispatch = "crates/keld-ipc/src/guard_dispatch.rs";
    let mutated = format!(
        "{}\nconst PROBE: &str = \"fs.write\";\n",
        source(&sources, dispatch)
    );
    assert_eq!(
        hits_in(
            &capability_hits(&with_file(sources.clone(), dispatch, mutated)),
            dispatch
        )
        .len(),
        1
    );

    // Prose and test-gated literals are not production string literals.
    let mutated = format!(
        "{}\n/// Evaluates `\"fs.read\"`.\n#[cfg(test)]\nmod probe {{\n    const P: &str = \"fs.read\";\n}}\n",
        source(&sources, dispatch)
    );
    assert!(
        hits_in(
            &capability_hits(&with_file(sources, dispatch, mutated)),
            dispatch
        )
        .is_empty()
    );
}

/// The exclusion engine fails closed: a file that any ungated declaration
/// reaches, a gated declaration nested in an inline module, and a `#[path`
/// form it cannot read all stay scanned. Unicode whitespace matches `\s`.
#[test]
fn exclusions_never_hide_a_production_file() {
    let literal = "const X: ChannelId = ChannelId(1);\n";
    let files = |sources: Sources| -> BTreeSet<String> {
        channel_hits(&sources)
            .into_iter()
            .map(|hit| hit.file)
            .collect()
    };
    let aliased = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "pub mod fs;\n#[cfg(test)]\n#[path = \"fs.rs\"]\nmod fs_again;\n",
        ),
        ("crates/x/src/fs.rs", literal),
    ]);
    assert_eq!(
        files(aliased),
        BTreeSet::from(["crates/x/src/fs.rs".to_owned()])
    );
    let nested = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "pub mod util;\npub mod a {\n    #[cfg(test)]\n    mod util;\n}\n",
        ),
        ("crates/x/src/util.rs", literal),
    ]);
    assert_eq!(
        files(nested),
        BTreeSet::from(["crates/x/src/util.rs".to_owned()])
    );
    let raw_path = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "#[cfg(test)]\n#[path = r\"t_tests.rs\"]\nmod t;\n",
        ),
        ("crates/x/src/t.rs", literal),
        ("crates/x/src/t_tests.rs", literal),
    ]);
    assert_eq!(
        files(raw_path),
        BTreeSet::from([
            "crates/x/src/t.rs".to_owned(),
            "crates/x/src/t_tests.rs".to_owned()
        ])
    );
    for spaced in [
        "ChannelId(\u{0B}1)",
        "ChannelId(\u{2003}1)",
        "const ECHO_CHANNEL:\u{A0}u16 = 1;",
    ] {
        let sources = synthetic(&[("crates/x/src/lib.rs", spaced)]);
        assert_eq!(channel_hits(&sources).len(), 1, "{spaced:?}");
    }
}

/// Criterion 1 in CI (the compile-fail doctest is local only): the table's
/// constructor stays private and nothing else constructs an entry.
#[test]
fn only_the_table_constructs_channel_entries() {
    let sources = real_sources();
    let table = source(&sources, TABLE);
    assert!(scan::entry_constructor_is_private(table));
    assert!(scan::entry_construction_hits(&sources, Path::new(TABLE)).is_empty());

    let public = replace_once(table, "    const fn new(", "    pub const fn new(");
    assert!(!scan::entry_constructor_is_private(&public));
    let second = format!("{table}\nimpl ChannelEntry {{\n    const fn new() {{}}\n}}\n");
    assert!(!scan::entry_constructor_is_private(&second));
    let native = "crates/keld-native/src/fs.rs";
    let minted = format!(
        "{}\nconst PROBE: ChannelEntry = ChannelEntry::new(\"probe\", 4, C, A);\n",
        source(&sources, native)
    );
    let hits = scan::entry_construction_hits(&with_file(sources, native, minted), Path::new(TABLE));
    assert_eq!(hits_in(&hits, native).len(), 1);
}

/// The macOS bridge's admitted id travels as a plain `u16` from `keld-core`,
/// which the two criterion 6 patterns cannot see; a positional literal at any
/// hop fails here instead (criteria 11 and 12).
const BRIDGE_ID_CALLS: &[(&str, usize)] = &[
    ("RendererBridgeEndpoint::new", 2),
    ("BridgeState::new", 1),
    ("render_bridge_script", 1),
];

#[test]
fn bridge_admitted_id_is_never_a_positional_literal() {
    let sources = real_sources();
    assert!(scan::positional_literal_hits(&sources, BRIDGE_ID_CALLS).is_empty());
    let session = "crates/keld-core/src/app_session.rs";
    let bridge = "crates/keld-wv/src/wkwebview/macos_bridge.rs";
    for (file, from, to) in [
        (
            session,
            "renderer_outcomes_rx,\n        keld_ipc::channel_table::ECHO.wire_id(),",
            "renderer_outcomes_rx,\n        1,",
        ),
        (
            bridge,
            "BridgeState::new(webview, admitted_channel)",
            "BridgeState::new(webview, 1)",
        ),
        (
            bridge,
            "render_bridge_script(PAGE_FACADE_SCRIPT, admitted_channel)",
            "render_bridge_script(PAGE_FACADE_SCRIPT, 1)",
        ),
    ] {
        let mutated = replace_once(source(&sources, file), from, to);
        let hits = scan::positional_literal_hits(
            &with_file(sources.clone(), file, mutated),
            BRIDGE_ID_CALLS,
        );
        assert_eq!(hits_in(&hits, file).len(), 1, "{to}");
    }
}

fn hit_files(sources: &Sources) -> BTreeSet<String> {
    channel_hits(sources)
        .into_iter()
        .map(|hit| hit.file)
        .collect()
}

fn set(files: &[&str]) -> BTreeSet<String> {
    files.iter().map(|file| (*file).to_owned()).collect()
}

/// Nested `mod x;` declarations resolve by Rust's module rules (the Reference,
/// "Modules"): through enclosing inline modules, under `<stem>/` for a
/// non-mod-rs file, and with `#[path]` inside an inline block relative to that
/// nested directory. A gated alias can never hide a production file.
#[test]
fn nested_module_declarations_resolve_by_rust_module_rules() {
    let literal = "const X: ChannelId = ChannelId(1);\n";
    let aliased = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "pub mod a {\n    pub mod util;\n}\n#[cfg(test)]\n#[path = \"a/util.rs\"]\nmod util_alias;\n",
        ),
        ("crates/x/src/a/util.rs", literal),
    ]);
    assert_eq!(hit_files(&aliased), set(&["crates/x/src/a/util.rs"]));

    let gated_nested = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "pub mod a {\n    #[cfg(test)]\n    mod t;\n}\nmod foo;\n",
        ),
        ("crates/x/src/a/t.rs", literal),
        ("crates/x/src/t.rs", literal),
        (
            "crates/x/src/foo.rs",
            "mod inner {\n    #[cfg(test)]\n    mod t;\n}\n",
        ),
        ("crates/x/src/foo/inner/t.rs", literal),
        ("crates/x/src/foo/t.rs", literal),
    ]);
    assert_eq!(
        hit_files(&gated_nested),
        set(&["crates/x/src/foo/t.rs", "crates/x/src/t.rs"])
    );

    let nested_path = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "mod a {\n    #[cfg(test)]\n    #[path = \"x_tests.rs\"]\n    mod x;\n}\n",
        ),
        ("crates/x/src/a/x_tests.rs", literal),
        ("crates/x/src/x_tests.rs", literal),
    ]);
    assert_eq!(hit_files(&nested_path), set(&["crates/x/src/x_tests.rs"]));

    // A `#[path]` inline module, or a production declaration the scanner cannot
    // resolve, fails closed: nothing out of line is excluded.
    let pathed_inline = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "#[path = \"other\"]\nmod a {\n    #[cfg(test)]\n    mod t;\n}\n",
        ),
        ("crates/x/src/a/t.rs", literal),
        ("crates/x/src/other/t.rs", literal),
    ]);
    assert_eq!(
        hit_files(&pathed_inline),
        set(&["crates/x/src/a/t.rs", "crates/x/src/other/t.rs"])
    );
    // An unresolvable production declaration is itself a hit at its line.
    let unresolved = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "#[path = r\"weird.rs\"]\nmod w;\n#[cfg(test)]\nmod t;\n",
        ),
        ("crates/x/src/t.rs", literal),
    ]);
    assert_eq!(
        channel_hits(&unresolved),
        vec![Hit {
            file: "crates/x/src/lib.rs".to_owned(),
            line: 2
        }]
    );
}

/// A file that production code `include!`s is production, even under `tests/`;
/// a test-gated `include!` site is not. An `include!` the scanner cannot read
/// fails closed at its call site.
#[test]
fn production_include_targets_are_scanned() {
    let literal = "const X: ChannelId = ChannelId(1);\n";
    let target = ("crates/x/tests/support/gen.rs", literal);
    let production = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "mod generated {\n    include!(\"../tests/support/gen.rs\");\n}\n",
        ),
        target,
    ]);
    assert_eq!(
        hit_files(&production),
        set(&["crates/x/tests/support/gen.rs"])
    );
    for gated in [
        "#[cfg(test)]\nmod t {\n    include!(\"../tests/support/gen.rs\");\n}\n",
        "#[cfg(test)]\ninclude!(\"../tests/support/gen.rs\");\n",
        "// include!(\"../tests/support/gen.rs\");\n",
    ] {
        let sources = synthetic(&[("crates/x/src/lib.rs", gated), target]);
        assert!(hit_files(&sources).is_empty(), "{gated}");
    }
    let aliased = synthetic(&[
        (
            "crates/x/src/lib.rs",
            "#[cfg(test)]\nmod t;\nmod p {\n    include!(\"t.rs\");\n}\n",
        ),
        ("crates/x/src/t.rs", literal),
    ]);
    assert_eq!(hit_files(&aliased), set(&["crates/x/src/t.rs"]));
    let unreadable = synthetic(&[(
        "crates/x/src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\n",
    )]);
    assert_eq!(
        channel_hits(&unreadable),
        vec![Hit {
            file: "crates/x/src/lib.rs".to_owned(),
            line: 1
        }]
    );

    // The loader reads a production include target from disk, wherever it is.
    let root = tempfile::tempdir().expect("fixture root");
    for (path, text) in [
        (
            "crates/x/src/lib.rs",
            "mod generated {\n    include!(\"../tests/gen.rs\");\n}\n",
        ),
        ("crates/x/tests/gen.rs", literal),
        ("crates/x/tests/unrelated.rs", literal),
    ] {
        let file = root.path().join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("fixture dir");
        std::fs::write(file, text).expect("fixture file");
    }
    std::fs::create_dir_all(root.path().join("crates/keld-ipc/fuzz/fuzz_targets"))
        .expect("fuzz dir");
    let loaded = scan::load_scan_inputs(root.path());
    assert!(loaded.contains_key(Path::new("crates/x/tests/gen.rs")));
    assert!(!loaded.contains_key(Path::new("crates/x/tests/unrelated.rs")));
    assert_eq!(hit_files(&loaded), set(&["crates/x/tests/gen.rs"]));
}

/// rustc treats `#[path]`-loaded files, crate roots other than `lib.rs` and
/// `main.rs`, and `include!`d files as mod-rs, so their nested `mod`
/// declarations resolve beside the file; a production declaration into
/// `tests/` is production too. None of these can be hidden by a gated alias.
/// Each layout was confirmed against rustc's own module loading in review.
#[test]
fn declarations_from_path_bin_and_included_files_stay_production() {
    let literal = "const X: ChannelId = ChannelId(1);\n";
    let gated_util = "mod a {\n    #[cfg(test)]\n    mod util;\n}\n";
    let path_root = format!("#[path = \"imp.rs\"]\nmod platform;\n{gated_util}");
    let include_root = format!("mod g {{\n    include!(\"gen.rs\");\n}}\n{gated_util}");
    let nested_a = "pub mod a {\n    pub mod util;\n}\n";
    let cases = [
        (
            "#[path] file declares a nested module",
            vec![
                ("crates/x/src/lib.rs", path_root.as_str()),
                ("crates/x/src/imp.rs", nested_a),
                ("crates/x/src/a/util.rs", literal),
            ],
            "crates/x/src/a/util.rs",
        ),
        (
            "included file declares a nested module",
            vec![
                ("crates/x/src/lib.rs", include_root.as_str()),
                ("crates/x/src/gen.rs", nested_a),
                ("crates/x/src/a/util.rs", literal),
            ],
            "crates/x/src/a/util.rs",
        ),
        (
            "bin crate root declares a sibling module",
            vec![
                (
                    "crates/x/src/lib.rs",
                    "#[cfg(test)]\n#[path = \"bin/common.rs\"]\nmod common_alias;\n",
                ),
                ("crates/x/src/bin/tool.rs", "mod common;\nfn main() {}\n"),
                ("crates/x/src/bin/common.rs", literal),
            ],
            "crates/x/src/bin/common.rs",
        ),
        (
            "production #[path] into tests/",
            vec![
                (
                    "crates/x/src/lib.rs",
                    "#[path = \"../tests/support/gen.rs\"]\nmod gen;\n",
                ),
                ("crates/x/tests/support/gen.rs", literal),
            ],
            "crates/x/tests/support/gen.rs",
        ),
        (
            "production module named tests",
            vec![
                ("crates/x/src/lib.rs", "pub mod tests;\n"),
                ("crates/x/src/tests/mod.rs", literal),
            ],
            "crates/x/src/tests/mod.rs",
        ),
    ];
    for (label, files, expected) in cases {
        assert_eq!(hit_files(&synthetic(&files)), set(&[expected]), "{label}");
    }
}

/// An `include!` whose target cannot be scanned fails closed at the call site:
/// a missing file, or a path that climbs above the repository root. The loader
/// follows production declarations as well as includes.
#[test]
fn unscannable_include_targets_are_hits() {
    for (text, line) in [
        ("mod g {\n    include!(\"absent.rs\");\n}\n", 2),
        ("include!(\"../../../../outside.rs\");\n", 1),
    ] {
        let sources = synthetic(&[("crates/x/src/lib.rs", text)]);
        assert_eq!(
            channel_hits(&sources),
            vec![Hit {
                file: "crates/x/src/lib.rs".to_owned(),
                line
            }],
            "{text}"
        );
    }

    let root = tempfile::tempdir().expect("fixture root");
    for (path, text) in [
        (
            "crates/x/src/lib.rs",
            "mod generated {\n    include!(\"../tests/gen.rs\");\n}\n",
        ),
        ("crates/x/tests/gen.rs", "pub mod helper;\n"),
        (
            "crates/x/tests/helper.rs",
            "const X: ChannelId = ChannelId(1);\n",
        ),
    ] {
        let file = root.path().join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("fixture dir");
        std::fs::write(file, text).expect("fixture file");
    }
    std::fs::create_dir_all(root.path().join("crates/keld-ipc/fuzz/fuzz_targets"))
        .expect("fuzz dir");
    let loaded = scan::load_scan_inputs(root.path());
    assert_eq!(hit_files(&loaded), set(&["crates/x/tests/helper.rs"]));
}
