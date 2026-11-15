//! v1 manifest rules on in-test fixtures (gh532 AC1–AC7; gh566 D3, C4, C10). Every
//! control mutates one field of the base manifest; the fixture derives a matching
//! denominator, so each mutation reaches the check it targets.

use keld_compat::evidence::Platform;
use serde_json::{Value, json};

use super::v1_fixture::{
    COMMIT, PAGE, QUIT, READY, SNAPSHOT, V0_COMMIT, V1, accepted, bytes, citation, edit, manifest,
    parse_bytes, parse_with, rejected, store,
};
use crate::corpus_manifest::{
    Corpus, CorpusError, LIFECYCLE_V0, Registration, V0_CORPUS_SHA256, sha256_uri,
};

fn is_rule(error: &CorpusError) -> bool {
    matches!(error, CorpusError::VerdictRule { .. })
}

/// gh532 AC1 and the scoped-pin control (gh566 D4): one v1 pin, no second pin form,
/// and each admitted pin serves only its scope.
#[test]
fn pin_admits_one_v1_pin_and_rejects_second_pins_and_foreign_scopes() {
    let corpus = accepted(&manifest());
    assert_eq!(
        (corpus.pin().version, corpus.pin().commit),
        ("44.4.5", COMMIT)
    );

    // A second upstream object: serde rejects the repeated field.
    let base = manifest();
    let text = String::from_utf8(bytes(&base)).expect("UTF-8");
    let doubled = text.replacen(
        "\"upstream\": {",
        &format!(
            "\"upstream\": {{ \"electron_version\": \"44.3.0\", \"electron_commit\": \"{V0_COMMIT}\" }},\n  \"upstream\": {{"
        ),
        1,
    );
    assert_ne!(doubled, text);
    let error = parse_bytes(&V1, doubled.as_bytes(), &base, &store()).expect_err("second pin");
    assert!(
        matches!(&error, CorpusError::Parse { message, .. } if message.contains("duplicate field `upstream`")),
        "{error}"
    );
    // An array of pins, and a cell-level commit (an unknown field).
    let array = edit(|m| m["upstream"] = json!([m["upstream"].clone(), m["upstream"].clone()]));
    assert!(matches!(rejected(&array), CorpusError::Parse { .. }));
    let cell_commit = edit(|m| m["cells"][0]["electron_commit"] = json!(COMMIT));
    assert!(matches!(
        &rejected(&cell_commit),
        CorpusError::Parse { message, .. } if message.contains("electron_commit")
    ));

    // The frozen 44.3.0 pin is scoped to electron-lifecycle-v0: a v1 corpus cannot use
    // it, and the v1 pair must match in both halves.
    for (version, commit) in [
        ("44.3.0", V0_COMMIT),
        ("44.4.5", V0_COMMIT),
        ("44.3.0", COMMIT),
    ] {
        let foreign = edit(|m| {
            m["upstream"] = json!({ "electron_version": version, "electron_commit": commit });
        });
        assert!(
            matches!(rejected(&foreign), CorpusError::UnadmittedPin { .. }),
            "{version} @ {commit}"
        );
    }

    // And the v1 pin is not admitted for the frozen shape.
    let (v0, _) = Corpus::fixture_bytes(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"));
    let text = String::from_utf8(v0).expect("UTF-8");
    let repinned = text
        .replacen(
            "\"electron_version\": \"44.3.0\"",
            "\"electron_version\": \"44.4.5\"",
            1,
        )
        .replacen(
            &format!("\"electron_commit\": \"{V0_COMMIT}\""),
            &format!("\"electron_commit\": \"{COMMIT}\""),
            1,
        );
    assert_ne!(repinned, text);
    let (_, v0_denominator) =
        Corpus::fixture_bytes(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"));
    let rehashed = String::from_utf8(v0_denominator)
        .expect("UTF-8")
        .replace(V0_CORPUS_SHA256, &sha256_uri(repinned.as_bytes()));
    let error = Corpus::parse(&LIFECYCLE_V0, repinned.as_bytes(), rehashed.as_bytes())
        .expect_err("v1 pin in the frozen shape");
    assert!(
        matches!(error, CorpusError::UnadmittedPin { .. }),
        "{error}"
    );
}

/// gh532 AC2 (cells): every v1 `oracle_id` carries the corpus pin's prefix.
#[test]
fn pin_requires_every_v1_oracle_at_the_corpus_pin() {
    let foreign = edit(|m| m["cells"][2]["oracle_id"] = json!("electron-v44.3.0.app.exit"));
    assert!(matches!(
        &rejected(&foreign),
        CorpusError::PinMismatch { cell, .. } if cell == "app.exit"
    ));
    let bare = edit(|m| m["cells"][0]["oracle_id"] = json!("electron-v44.4.5."));
    assert!(matches!(rejected(&bare), CorpusError::PinMismatch { .. }));
}

/// gh532 AC3: the URL is a docs page at the corpus commit, and `quote_sha256` is the
/// digest of the quote.
#[test]
fn citation_url_and_quote_digest_bind_the_pin() {
    let url_with = |url: String| edit(move |m| m["cells"][0]["doc_citation"]["url"] = json!(url));
    let blob = format!("https://github.com/electron/electron/blob/{COMMIT}/");
    for (label, url) in [
        (
            "other commit",
            format!("https://github.com/electron/electron/blob/{V0_COMMIT}/{PAGE}"),
        ),
        ("not under docs", format!("{blob}README.md")),
        ("query", format!("{blob}{PAGE}?plain=1")),
        ("dot-dot", format!("{blob}docs/../{PAGE}")),
        ("empty segment", format!("{blob}docs//api/app.md")),
        ("not markdown", format!("{blob}docs/api/app.txt")),
        ("unsafe anchor", format!("{blob}{PAGE}#bad anchor")),
        ("empty anchor", format!("{blob}{PAGE}#")),
        (
            "other host",
            format!("https://github.com/electron/electronx/blob/{COMMIT}/{PAGE}"),
        ),
    ] {
        assert!(
            matches!(rejected(&url_with(url)), CorpusError::CitationUrl { .. }),
            "{label}"
        );
    }
    accepted(&url_with(format!("{blob}{PAGE}")));

    // One byte edited in the quote, without a new digest: it is no longer in the page.
    let edited = READY.replacen('.', ",", 1);
    let one_byte = edit(|m| m["cells"][0]["doc_citation"]["quote"] = json!(edited));
    assert!(matches!(
        rejected(&one_byte),
        CorpusError::QuoteAbsent { .. }
    ));
    // A quote that is in the page but carries another quote's digest.
    let wrong_digest = edit(|m| {
        m["cells"][0]["doc_citation"]["quote_sha256"] = json!(sha256_uri(QUIT.as_bytes()));
    });
    assert!(matches!(
        rejected(&wrong_digest),
        CorpusError::QuoteDigestMismatch { .. }
    ));
    let empty = edit(|m| {
        m["cells"][0]["doc_citation"] =
            json!({ "url": format!("{blob}{PAGE}"), "quote": "", "quote_sha256": sha256_uri(b"") });
    });
    assert!(matches!(rejected(&empty), CorpusError::CitationUrl { .. }));
    let second_kind = edit(|m| m["cells"][0]["doc_citation"]["source_receipt"] = json!("x"));
    assert!(matches!(rejected(&second_kind), CorpusError::Parse { .. }));
}

/// gh532 AC4 and AC5: a `fail` cell carries exactly one key; a pass cell carries none;
/// a ticket is canonical. Deleting the ticket from the red cell is the fourth issue
/// control.
#[test]
fn red_cell_needs_exactly_one_key_and_a_canonical_ticket() {
    let base = accepted(&manifest());
    assert_eq!(
        base.cells()[1].implementing_ticket.as_deref(),
        Some("GH-445")
    );
    assert!(
        base.cells()[2].divergence.is_some(),
        "a divergence-only cell is admitted"
    );

    let deleted = edit(|m| {
        m["cells"][1]
            .as_object_mut()
            .expect("cell")
            .remove("implementing_ticket");
    });
    assert!(is_rule(&rejected(&deleted)), "a fail cell with neither key");
    let both = edit(|m| m["cells"][1]["intentional_divergence"] = json!("both keys"));
    assert!(is_rule(&rejected(&both)));
    for ticket in ["GH-0445", "JIRA-445", "GH-", "gh-445", "KEL-12a"] {
        let bad = edit(|m| m["cells"][1]["implementing_ticket"] = json!(ticket));
        assert!(is_rule(&rejected(&bad)), "{ticket}");
    }
    accepted(&edit(|m| {
        m["cells"][1]["implementing_ticket"] = json!("KEL-237");
    }));
    let flipped_with_ticket = edit(|m| m["cells"][0]["implementing_ticket"] = json!("GH-445"));
    assert!(is_rule(&rejected(&flipped_with_ticket)), "gh532 AC5");
    let pass_divergence = edit(|m| m["cells"][0]["intentional_divergence"] = json!("x"));
    assert!(is_rule(&rejected(&pass_divergence)));
    let blank = edit(|m| m["cells"][2]["intentional_divergence"] = json!("   "));
    assert!(is_rule(&rejected(&blank)));
    let waived = edit(|m| m["cells"][0]["expected_verdict"] = json!("waived"));
    assert!(is_rule(&rejected(&waived)));
}

/// gh532 AC6: an uncited cell is `unknown`; an `unknown` cell carries no key, and may
/// carry a citation, which is then verified.
#[test]
fn uncited_cells_are_unknown() {
    let uncite = |index: usize, verdict: &str| {
        let verdict = verdict.to_owned();
        edit(move |m| {
            m["cells"][index]
                .as_object_mut()
                .expect("cell")
                .remove("doc_citation");
            m["cells"][index]["expected_verdict"] = json!(verdict);
        })
    };
    assert!(is_rule(&rejected(&uncite(0, "pass"))));
    assert!(is_rule(&rejected(&uncite(2, "fail"))));
    let keyed_unknown = edit(|m| m["cells"][3]["implementing_ticket"] = json!("GH-445"));
    assert!(is_rule(&rejected(&keyed_unknown)));
    let cited_unknown = edit(|m| m["cells"][3]["doc_citation"] = citation(READY));
    accepted(&cited_unknown);
    let bad_cited_unknown = edit(|m| m["cells"][3]["doc_citation"] = citation("not in the page"));
    assert!(matches!(
        rejected(&bad_cited_unknown),
        CorpusError::QuoteAbsent { .. }
    ));
}

/// gh532 AC7 and gh566 D3: `artifact_digest` has one meaning, and `schema` is closed.
#[test]
fn digest_and_schema_have_one_meaning() {
    let missing = edit(|m| {
        m.as_object_mut()
            .expect("manifest")
            .remove("artifact_digest");
    });
    assert!(matches!(
        &rejected(&missing),
        CorpusError::Parse { message, .. } if message.contains("artifact_digest")
    ));
    let other = edit(|m| m["artifact_digest"] = json!("fixture_set"));
    assert!(matches!(
        rejected(&other),
        CorpusError::UnsupportedArtifactDigest { .. }
    ));
    let no_schema = edit(|m| {
        m.as_object_mut().expect("manifest").remove("schema");
    });
    assert!(matches!(rejected(&no_schema), CorpusError::Parse { .. }));
    let other_schema = edit(|m| m["schema"] = json!("keld.compat.corpus/v2"));
    assert!(matches!(
        rejected(&other_schema),
        CorpusError::UnknownSchema { .. }
    ));

    // One flipped byte breaks the digest binding (the denominator keeps the old digest).
    let base = manifest();
    let good = bytes(&base);
    let mut flipped = good.clone();
    let index = flipped.iter().position(|byte| *byte == b'e').expect("an e");
    flipped[index] = b'E';
    let denominator = super::v1_fixture::denominator(&good, &base);
    let reader = |_: &str| Err(std::io::Error::from(std::io::ErrorKind::NotFound));
    let error = Corpus::parse_with_snapshots(&V1, &flipped, &denominator, &reader)
        .expect_err("flipped byte");
    assert!(
        matches!(error, CorpusError::DigestMismatch { .. }),
        "{error}"
    );

    // The frozen lifecycle bytes are not a v1 manifest.
    let renamed = Registration {
        shape: crate::corpus_manifest::Shape::V1,
        ..LIFECYCLE_V0
    };
    let (v0, v0_denominator) =
        Corpus::fixture_bytes(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        Corpus::parse(&renamed, &v0, &v0_denominator),
        Err(CorpusError::Parse { .. })
    ));
}

/// gh566 C4: a repeated `engine` or `doc_snapshots` key is rejected, not collapsed.
#[test]
fn duplicate_keys_are_rejected() {
    let base = manifest();
    let text = String::from_utf8(bytes(&base)).expect("UTF-8");
    for (field, from, to) in [
        (
            "engine",
            "\"macos\": \"headless-lifecycle-conformance\"".to_owned(),
            "\"macos\": \"headless-lifecycle-conformance\",\n    \"macos\": \"wkwebview\""
                .to_owned(),
        ),
        (
            "doc_snapshots",
            format!("\"{PAGE}\": \"{}\"", sha256_uri(SNAPSHOT.as_bytes())),
            format!(
                "\"{PAGE}\": \"{}\",\n    \"{PAGE}\": \"{}\"",
                sha256_uri(b"other"),
                sha256_uri(SNAPSHOT.as_bytes())
            ),
        ),
    ] {
        let repeated = text.replacen(&from, &to, 1);
        assert_ne!(repeated, text, "{field}");
        let error =
            parse_bytes(&V1, repeated.as_bytes(), &base, &store()).expect_err("repeated key");
        assert!(
            matches!(&error, CorpusError::DuplicateKey { field: found, .. } if *found == field),
            "{field}: {error}"
        );
    }
}

/// gh566 C10 (parse): `platforms` is non-empty, distinct, known and registered.
#[test]
fn platforms_are_declared_distinct_known_and_registered() {
    let with = |platforms: Value| edit(move |m| m["cells"][0]["platforms"] = platforms);
    assert!(matches!(
        rejected(&with(json!([]))),
        CorpusError::EmptyPlatforms { .. }
    ));
    for platforms in [
        json!(["macos", "macos"]),
        json!(["freebsd"]),
        json!(["MacOS"]),
    ] {
        assert!(
            matches!(
                rejected(&with(platforms.clone())),
                CorpusError::InvalidPlatforms { .. }
            ),
            "{platforms}"
        );
    }
    let missing = edit(|m| {
        m["cells"][0]
            .as_object_mut()
            .expect("cell")
            .remove("platforms");
    });
    assert!(matches!(rejected(&missing), CorpusError::Parse { .. }));

    let mac_only = Registration {
        platforms: &[Platform::Macos],
        ..V1
    };
    let error = parse_with(&mac_only, &manifest(), &store()).expect_err("unregistered lane");
    assert!(
        matches!(error, CorpusError::InvalidPlatforms { .. }),
        "{error}"
    );
    let corpus = accepted(&manifest());
    assert_eq!(corpus.cells()[1].platforms, vec![Platform::Macos]);
}

/// gh532 AC9 (static): the `engine` map names platforms, each with one identity token.
#[test]
fn engine_map_names_platforms_with_one_identity() {
    for engine in [
        json!({}),
        json!({ "freebsd": "headless-lifecycle-conformance" }),
        json!({ "macos": "wkwebview@1" }),
        json!({ "macos": "" }),
        json!({ "macos": "web view" }),
    ] {
        let bad = edit(|m| m["engine"] = engine.clone());
        assert!(
            matches!(rejected(&bad), CorpusError::InvalidEngine { .. }),
            "{engine}"
        );
    }
    let corpus = accepted(&edit(|m| m["engine"] = json!({ "macos": "wkwebview" })));
    assert_eq!(corpus.engine_token(Platform::Macos), Some("wkwebview"));
    assert_eq!(corpus.engine_token(Platform::Linux), None);
}
