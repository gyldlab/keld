//! Fixture-driven authority-boundary tests for `keld-guard`.
#![allow(
    clippy::expect_used,
    reason = "Clippy does not classify Cargo integration-test crates as tests for allow-expect-in-tests"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use keld_guard::{
    Decision, DenyReason, ManifestError, PermissionsManifest, Principal, evaluate, load_manifest,
    parse_manifest,
};

#[derive(Debug, Clone, Copy)]
enum ExpectedDeny {
    NotGranted,
    OutOfScope,
}

#[derive(Debug, Clone, Copy)]
enum ExpectedDecision {
    Allow,
    Deny {
        variant: ExpectedDeny,
        code: &'static str,
        kind: &'static str,
    },
}

#[derive(Debug, Clone, Copy)]
struct Case {
    name: &'static str,
    operation: &'static str,
    path: &'static str,
    expected: ExpectedDecision,
}

const ALLOW: ExpectedDecision = ExpectedDecision::Allow;
const NOT_GRANTED: ExpectedDecision = ExpectedDecision::Deny {
    variant: ExpectedDeny::NotGranted,
    code: "KELD-GUARD001",
    kind: "not_granted",
};
const OUT_OF_SCOPE: ExpectedDecision = ExpectedDecision::Deny {
    variant: ExpectedDeny::OutOfScope,
    code: "KELD-GUARD002",
    kind: "out_of_scope",
};

const CASES: &[Case] = &[
    Case {
        name: "exact path",
        operation: "fs.read",
        path: "/foo/exact",
        expected: ALLOW,
    },
    Case {
        name: "exact path child",
        operation: "fs.read",
        path: "/foo/exact/child",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "prefix root",
        operation: "fs.read",
        path: "/foo/bar",
        expected: ALLOW,
    },
    Case {
        name: "prefix child",
        operation: "fs.read",
        path: "/foo/bar/child",
        expected: ALLOW,
    },
    Case {
        name: "sibling prefix",
        operation: "fs.read",
        path: "/foo/barista/secret",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "slash traversal",
        operation: "fs.read",
        path: "/foo/bar/../secret",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "backslash traversal",
        operation: "fs.read",
        path: r"/foo/bar\..\secret",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "dot segment inside prefix",
        operation: "fs.read",
        path: "/foo/bar/./child",
        expected: ALLOW,
    },
    Case {
        name: "dot segment before prefix",
        operation: "fs.read",
        path: "/foo/./bar/child",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "repeated separator inside prefix",
        operation: "fs.read",
        path: "/foo/bar//child",
        expected: ALLOW,
    },
    Case {
        name: "repeated separator before prefix",
        operation: "fs.read",
        path: "/foo//bar/child",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "variable-looking literal",
        operation: "fs.read",
        path: "$APPDATA/notes.db",
        expected: ALLOW,
    },
    Case {
        name: "resolved-looking variable value",
        operation: "fs.read",
        path: "/Users/example/AppData/notes.db",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "line-comment marker inside string",
        operation: "fs.read",
        path: "https://example.com/a//b",
        expected: ALLOW,
    },
    Case {
        name: "block-comment marker inside string",
        operation: "fs.read",
        path: "/foo/*literal*/bar",
        expected: ALLOW,
    },
    Case {
        name: "url authority swallowed by scheme glob",
        operation: "net.connect",
        path: "https://evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "url authority swallowed by scheme glob with path",
        operation: "net.connect",
        path: "https://evil.example.com/steal",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "url authority swallowed by wss scheme glob",
        operation: "net_wss.connect",
        path: "wss://attacker.example/ws",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "url origin prefix grant",
        operation: "net_origin.connect",
        path: "https://api.example.com/v1",
        expected: ALLOW,
    },
    Case {
        name: "url origin root itself",
        operation: "net_origin.connect",
        path: "https://api.example.com",
        expected: ALLOW,
    },
    Case {
        name: "url sibling authority beyond origin prefix",
        operation: "net_origin.connect",
        path: "https://api.example.com.evil.test/v1",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "url authority swallowed by single-slash scheme glob",
        operation: "net_single_slash.connect",
        path: "https:/evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "url authority swallowed by triple-slash scheme glob",
        operation: "net_triple_slash.connect",
        path: "https:///evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "windows drive glob keeps path semantics",
        operation: "shell.open",
        path: "C://Users/app/x",
        expected: ALLOW,
    },
    Case {
        name: "windows drive glob does not reach another drive",
        operation: "shell.open",
        path: "E:/other/x",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "backslash at the grant boundary is not the anchor",
        operation: "anchor.read",
        path: r"/foo/bar\child",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "forward slash at the grant boundary is the anchor",
        operation: "anchor.read",
        path: "/foo/bar/child",
        expected: ALLOW,
    },
    Case {
        name: "one letter scheme glob with separators",
        operation: "net_one_letter.connect",
        path: "a://evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "backslash scheme glob",
        operation: "net_backslash.connect",
        path: "https:\\/evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "tab separated scheme glob",
        operation: "net_tab.connect",
        path: "https:\t//evil.example.com",
        expected: OUT_OF_SCOPE,
    },
    Case {
        name: "colon outside scheme position keeps its path grant",
        operation: "fs2.read",
        path: "/srv/backup:/x",
        expected: ALLOW,
    },
    Case {
        name: "windows device prefixed drive grant",
        operation: "fs2.read",
        path: "\\\\?\\C:/Users/x",
        expected: ALLOW,
    },
    Case {
        name: "unknown operation",
        operation: "fs.delete",
        path: "/foo/bar/child",
        expected: NOT_GRANTED,
    },
    Case {
        name: "empty grant",
        operation: "fs.write",
        path: "/foo/bar/child",
        expected: NOT_GRANTED,
    },
];

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn load_fixture(name: &str) -> PermissionsManifest {
    let path = fixture_path(name);
    load_manifest(&path).expect("checked ACL fixture must load")
}

/// The one rendering of a decision as snapshot text.
///
/// `assert_case` and `decision_snapshot` both emit it — the asserting path for the
/// checked-in matrix, the comparing path for a reduced fixture — so it has one owner.
fn decision_text(decision: &Decision) -> String {
    match decision {
        Decision::Allow => "allow".to_owned(),
        Decision::Deny(reason) => format!("deny {} {}", reason.code(), reason.kind()),
    }
}

fn assert_case(manifest: &PermissionsManifest, case: Case) -> String {
    let actual = evaluate(manifest, Principal::AppProcess, case.operation, case.path);
    match (case.expected, actual) {
        (ExpectedDecision::Allow, Decision::Allow) => "allow".to_owned(),
        (
            ExpectedDecision::Deny {
                variant,
                code,
                kind,
            },
            Decision::Deny(reason),
        ) => {
            assert_eq!(reason.code(), code, "{} returned the wrong code", case.name);
            assert_eq!(reason.kind(), kind, "{} returned the wrong kind", case.name);
            let expected_variant = matches!(
                (variant, &reason),
                (ExpectedDeny::NotGranted, DenyReason::NotGranted { .. })
                    | (ExpectedDeny::OutOfScope, DenyReason::OutOfScope { .. })
            );
            assert!(
                expected_variant,
                "{} returned the wrong denial variant: {reason:?}",
                case.name
            );
            decision_text(&Decision::Deny(reason))
        }
        (ExpectedDecision::Allow, actual) => {
            assert_eq!(
                actual,
                Decision::Allow,
                "{} must match its independent allow expectation",
                case.name
            );
            "allow".to_owned()
        }
        (expected @ ExpectedDecision::Deny { .. }, actual) => {
            assert!(
                matches!(actual, Decision::Deny(_)),
                "{} violated its independent expected decision {expected:?}: {actual:?}",
                case.name
            );
            String::new()
        }
    }
}

#[test]
fn fixture_decision_matrix_matches_authority_contract() {
    let manifest = load_fixture("scopes.jsonc");
    let mut observed = String::new();

    for &case in CASES {
        let decision = assert_case(&manifest, case);
        writeln!(&mut observed, "{}: {decision}", case.name)
            .expect("writing to String cannot fail");
    }

    assert_eq!(
        observed,
        include_str!("fixtures/scopes.expected"),
        "the checked decision snapshot must change only with an intentional authority-contract change"
    );
}

/// Decides every case in the matrix against `manifest`, as one comparable snapshot.
///
/// Unlike `assert_case` this never panics: it is used to compare a reduced fixture
/// against the real one, where decisions are *expected* to move.
fn decision_snapshot(manifest: &PermissionsManifest) -> Vec<String> {
    CASES
        .iter()
        .map(|case| {
            decision_text(&evaluate(
                manifest,
                Principal::AppProcess,
                case.operation,
                case.path,
            ))
        })
        .collect()
}

/// Every grant row in the fixture must change at least one decision when removed.
///
/// KEL-208 shipped two rows that did not: one whose JSONC escaping made it a different
/// grant than its comment claimed, and one fully shadowed by a broader entry in the same
/// list. Both looked like coverage and pinned nothing, which is worse than no row at all
/// — `crates/keld-guard/AGENTS.md` requires this fixture to be a permanent bypass record.
///
/// Rows are dropped by line, so this test needs no knowledge of how a scope is escaped;
/// that is exactly the knowledge the original defect got wrong.
/// Whether a fixture line is a bare string element of a scope array.
fn is_grant_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('"') && (trimmed.ends_with('"') || trimmed.ends_with("\","))
}

/// The fixture text without `index`, kept parseable.
///
/// Dropping the last element of an array strands the previous element's comma before
/// the `]`. Repairing it matters: skipping those rows instead would leave the last
/// entry of every array unchecked, which is the silent-coverage failure this contract
/// exists to catch.
fn without_row(lines: &[&str], index: usize) -> String {
    let mut kept: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(other, _)| *other != index)
        .map(|(_, line)| (*line).to_owned())
        .collect();
    let closes_array = kept
        .get(index)
        .is_some_and(|line| line.trim_start().starts_with(']'));
    if closes_array && index > 0 {
        for previous in (0..index).rev() {
            let visible = kept[previous].trim_end().len();
            if kept[previous][..visible].ends_with(',') {
                // `,` is ASCII, so `visible - 1` is a char boundary.
                kept[previous].truncate(visible - 1);
                break;
            }
            if visible != 0 {
                break;
            }
        }
    }
    kept.join("\n")
}

#[test]
fn every_fixture_grant_row_is_load_bearing() {
    let text = include_str!("fixtures/scopes.jsonc");
    let baseline = decision_snapshot(&load_fixture("scopes.jsonc"));

    let lines: Vec<&str> = text.split('\n').collect();
    let rows: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_grant_row(line))
        .map(|(index, _)| index)
        .collect();

    let mut checked = 0usize;
    for &index in &rows {
        let reduced = parse_manifest(&without_row(&lines, index)).unwrap_or_else(|error| {
            panic!(
                "dropping line {} must still yield valid JSONC: {error}",
                index + 1
            )
        });
        checked += 1;
        assert_ne!(
            decision_snapshot(&reduced),
            baseline,
            "removing line {} of scopes.jsonc ({}) changes no decision, so the row pins nothing. Give it its own capability, or delete it.",
            index + 1,
            lines[index].trim()
        );
    }
    // A row skipped for any reason is a row this contract did not cover.
    assert_eq!(
        checked,
        rows.len(),
        "every detected grant row must be checked, not skipped"
    );
    assert!(
        rows.len() >= 10,
        "expected the fixture to contribute many grant rows, found {}",
        rows.len()
    );
}

#[test]
fn empty_manifest_defaults_to_not_granted() {
    let manifest = load_fixture("empty.jsonc");
    let requested = "/foo/bar/notes.txt";
    let decision = evaluate(&manifest, Principal::AppProcess, "fs.read", requested);

    match decision {
        Decision::Deny(DenyReason::NotGranted {
            capability,
            json_pointer,
            requested: denied_path,
        }) => {
            assert_eq!(capability, "fs.read");
            assert_eq!(json_pointer, "/app/fs/read");
            assert_eq!(denied_path, requested);
        }
        other => assert!(
            matches!(other, Decision::Deny(DenyReason::NotGranted { .. })),
            "empty manifest must default-deny fs.read, got {other:?}"
        ),
    }

    assert_eq!(
        assert_case(&load_fixture("scopes.jsonc"), CASES[0]),
        "allow",
        "the paired allow proves this test is not satisfied by unconditional denial"
    );
}

#[test]
fn missing_and_malformed_manifests_fail_closed() {
    let missing_path = fixture_path("does-not-exist.jsonc");
    let missing = load_manifest(&missing_path).expect_err("missing fixture must fail closed");
    assert_eq!(
        missing,
        ManifestError::NotFound {
            path: missing_path.clone()
        }
    );
    let missing_message = missing.to_string();
    assert!(
        missing_message.contains("KELD-GUARD004"),
        "{missing_message}"
    );
    assert!(
        missing_message.contains(&missing_path.display().to_string()),
        "{missing_message}"
    );

    let malformed_path = fixture_path("malformed.jsonc");
    let malformed = load_manifest(&malformed_path).expect_err("malformed JSONC must fail closed");
    match &malformed {
        ManifestError::Parse {
            path: Some(path), ..
        } => assert_eq!(path, &malformed_path),
        other => assert!(
            matches!(other, ManifestError::Parse { .. }),
            "malformed fixture returned the wrong error: {other:?}"
        ),
    }
    let malformed_message = malformed.to_string();
    assert!(
        malformed_message.contains("KELD-GUARD005"),
        "{malformed_message}"
    );
    assert!(
        malformed_message.contains("Fix the JSON"),
        "{malformed_message}"
    );
}

#[test]
fn every_denial_reason_variant_has_a_stable_contract() {
    let not_granted = DenyReason::NotGranted {
        capability: "fs.delete".to_owned(),
        json_pointer: "/app/fs/delete".to_owned(),
        requested: "/foo/bar/notes.txt".to_owned(),
    };
    assert_eq!(not_granted.code(), "KELD-GUARD001");
    assert_eq!(not_granted.kind(), "not_granted");
    assert_eq!(
        not_granted.fix(),
        "Append \"/foo/bar/notes.txt\" to `/app/fs/delete` in keld.permissions.jsonc."
    );
    assert_eq!(
        not_granted.to_string(),
        "KELD-GUARD001: capability `fs.delete` is not granted. \
         Append \"/foo/bar/notes.txt\" to `/app/fs/delete` in keld.permissions.jsonc."
    );

    let out_of_scope = DenyReason::OutOfScope {
        capability: "fs.read".to_owned(),
        scope: "/foo/bar/**".to_owned(),
        json_pointer: "/app/fs/read".to_owned(),
        requested: "/foo/barista/secret".to_owned(),
    };
    assert_eq!(out_of_scope.code(), "KELD-GUARD002");
    assert_eq!(out_of_scope.kind(), "out_of_scope");
    assert_eq!(
        out_of_scope.fix(),
        "Widen `/app/fs/read` in keld.permissions.jsonc so it includes `/foo/barista/secret`."
    );
    assert_eq!(
        out_of_scope.to_string(),
        "KELD-GUARD002: capability `fs.read` denied by scope `/foo/bar/**`. \
         Widen `/app/fs/read` in keld.permissions.jsonc so it includes `/foo/barista/secret`."
    );

    // Channel evaluation is not in the v0 public API. Pin the exposed denial
    // contract without pretending this fixture exercises a nonexistent matcher.
    let channel_forbidden = DenyReason::ChannelForbidden {
        channel: "notes.unknown".to_owned(),
    };
    assert_eq!(channel_forbidden.code(), "KELD-GUARD003");
    assert_eq!(channel_forbidden.kind(), "channel_forbidden");
    assert_eq!(
        channel_forbidden.fix(),
        "Add `notes.unknown` to this principal's channels list in keld.permissions.jsonc."
    );
    assert_eq!(
        channel_forbidden.to_string(),
        "KELD-GUARD003: channel `notes.unknown` is not granted to this principal. \
         Add `notes.unknown` to this principal's channels list in keld.permissions.jsonc."
    );

    let not_app = DenyReason::NotAppProcess {
        principal: Principal::Webview {
            id: 1,
            generation: 2,
        },
    };
    assert_eq!(not_app.code(), "KELD-GUARD006");
    assert_eq!(not_app.kind(), "not_app_process");
    assert!(
        !not_app.fix().contains("/app/"),
        "must not recommend applying app scopes: {}",
        not_app.fix()
    );
    let not_app_msg = not_app.to_string();
    assert!(not_app_msg.contains("KELD-GUARD006"), "{not_app_msg}");
    assert!(not_app_msg.contains("webview"), "{not_app_msg}");

    let media = DenyReason::MediaPrincipalRequired {
        capability: "web.camera".to_owned(),
        presented: Some(Principal::AppProcess),
    };
    assert_eq!(media.code(), "KELD-GUARD007");
    assert_eq!(media.kind(), "media_principal_required");
    assert_eq!(
        media.fix(),
        "Mint the requesting webview principal before evaluating `web.camera`. \
         Do not present AppProcess — that would apply `/app` media grants to every webview."
    );
    assert_eq!(
        media.to_string(),
        "KELD-GUARD007: `web.camera` requires a minted webview principal (presented `app`). \
         Mint the requesting webview principal before evaluating `web.camera`. \
         Do not present AppProcess — that would apply `/app` media grants to every webview."
    );
    assert!(
        !media.fix().contains("/app/web"),
        "must not recommend applying app media grants: {}",
        media.fix()
    );
}

#[test]
fn in_scope_app_path_does_not_allow_webview_principal() {
    let manifest = load_fixture("scopes.jsonc");
    let webview = Principal::Webview {
        id: 1,
        generation: 1,
    };
    match evaluate(&manifest, webview, "fs.read", "/foo/exact") {
        Decision::Deny(DenyReason::NotAppProcess { principal }) => {
            assert_eq!(principal, webview);
        }
        other => panic!("in-scope app path must not allow a webview principal: {other:?}"),
    }
}
