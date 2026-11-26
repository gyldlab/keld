//! Reject untrusted evidence locations and accept immutable, pinned evidence.

use super::{
    AS_OF, DIGEST_A, Denominator, parse_denominator, parse_evidence, parse_ok, score,
    valid_denominator_json, valid_evidence_json,
};

#[test]
fn rejects_mutable_https_without_content_address() {
    for uri in [
        "https://example.com/foo",
        "https://example.com/tmp/not-this",
        "https://github.com/gyldlab/keld/blob/main/README.md",
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        assert!(err.to_string().contains("non-normative leads"), "{uri}");
    }
}

#[test]
fn rejects_loopback_userinfo_and_unspecified_https_hosts() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    for uri in [
        format!("https://git@127.0.0.1/org/repo/commit/{sha}"),
        format!("https://[::ffff:127.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:7f00:1]/org/repo/commit/{sha}"),
        format!("https://0.0.0.0/org/repo/commit/{sha}"),
        format!("https://127.0.0.1/org/repo/commit/{sha}"),
        format!("https://[::1]/org/repo/commit/{sha}"),
        format!("https://user@github.com/gyldlab/keld/commit/{sha}"),
        format!("https://[::]/org/repo/commit/{sha}"),
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

#[test]
fn rejects_rfc1918_link_local_and_unique_local_https_hosts() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    for uri in [
        format!("https://10.0.0.1/org/repo/commit/{sha}"),
        format!("https://192.168.1.1/org/repo/commit/{sha}"),
        format!("https://172.16.0.1/org/repo/commit/{sha}"),
        format!("https://169.254.0.1/org/repo/commit/{sha}"),
        format!("https://[fc00::1]/org/repo/commit/{sha}"),
        format!("https://[fd12:3456:789a::1]/org/repo/commit/{sha}"),
        format!("https://[fe80::1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:c0a8:101]/org/repo/commit/{sha}"),
        format!("https://10.0.0.1./org/repo/commit/{sha}"),
        format!("https://192.168.1.1./org/repo/commit/{sha}"),
        format!("https://169.254.0.1./org/repo/commit/{sha}"),
        format!("https://[::10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::a00:1]/org/repo/commit/{sha}"),
        format!("https://[0:0:0:0:0:0:10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::169.254.0.1]/org/repo/commit/{sha}"),
        format!("https://[::a9fe:1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:0:10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:0:a00:1]/org/repo/commit/{sha}"),
        format!("https://localhost./org/repo/commit/{sha}"),
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        assert!(
            err.to_string().contains("not a public https location"),
            "{uri}: {err}"
        );
    }
    let public_ipv4 = format!("https://1.1.1.1/org/repo/commit/{sha}");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &public_ipv4,
    );
    parse_evidence(json.as_bytes()).expect("public unicast IPv4 is a public https host");
    let public_fqdn_dot = format!("https://1.1.1.1./org/repo/commit/{sha}");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &public_fqdn_dot,
    );
    parse_evidence(json.as_bytes())
        .expect("trailing FQDN dot on a public IPv4 is still a public host");
    let pinned = format!("https://github.com/gyldlab/keld/blob/{sha}/README.md");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &pinned,
    );
    parse_evidence(json.as_bytes()).expect("commit-pinned blob is still a pin");
}

#[test]
fn rejects_nat64_6to4_and_cgnat_https_hosts() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    for uri in [
        format!("https://[64:ff9b::10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[2002:a00:1::1]/org/repo/commit/{sha}"),
        format!("https://100.64.0.1/org/repo/commit/{sha}"),
        format!("https://100.64.0.1./org/repo/commit/{sha}"),
        format!("https://[::ffff:100.64.0.1]/org/repo/commit/{sha}"),
        format!("https://[64:ff9b::1.1.1.1]/org/repo/commit/{sha}"),
        format!("https://[2002:808:808::1]/org/repo/commit/{sha}"),
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        assert!(
            err.to_string().contains("not a public https location"),
            "{uri}: {err}"
        );
    }
}

#[test]
fn accepts_sha256_evidence_uri() {
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        DIGEST_A,
    );
    let record = parse_evidence(json.as_bytes()).expect("sha256 uri");
    assert_eq!(record.evidence_uri, DIGEST_A);
}

#[test]
fn rejects_blob_main_even_when_a_hex_path_segment_is_present() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for uri in [
        format!("https://github.com/gyldlab/keld/blob/main/{hexfile}"),
        format!("https://github.com/gyldlab/keld/tree/main/{hexfile}"),
        format!("https://github.com/gyldlab/keld/BLOB/MAIN/{hexfile}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/main/{hexfile}"),
    ] {
        let json = valid_evidence_json().replace(
            "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
            &uri,
        );
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
    let pinned = format!("https://github.com/gyldlab/keld/blob/{sha}/README.md");
    let json = valid_evidence_json().replace(
        "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493",
        &pinned,
    );
    parse_evidence(json.as_bytes()).expect("commit-pinned blob is a pin");
}

#[test]
fn score_rejects_constructed_loopback_and_blob_main_uris() {
    let mut denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denom");
    denom.cells.truncate(1);
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let hexfile = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for uri in [
        format!("https://git@127.0.0.1/org/repo/commit/{sha}"),
        format!("https://[::ffff:127.0.0.1]/org/repo/commit/{sha}"),
        format!("https://0.0.0.0/org/repo/commit/{sha}"),
        "https://github.com/gyldlab/keld/blob/main/README.md".to_owned(),
        format!("https://github.com/gyldlab/keld/blob/main/{hexfile}"),
        format!("https://10.0.0.1/org/repo/commit/{sha}"),
        format!("https://192.168.1.1/org/repo/commit/{sha}"),
        format!("https://172.16.0.1/org/repo/commit/{sha}"),
        format!("https://169.254.0.1/org/repo/commit/{sha}"),
        format!("https://[fc00::1]/org/repo/commit/{sha}"),
        format!("https://10.0.0.1./org/repo/commit/{sha}"),
        format!("https://[::10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[::ffff:0:10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://localhost./org/repo/commit/{sha}"),
        format!("https://[64:ff9b::10.0.0.1]/org/repo/commit/{sha}"),
        format!("https://[2002:a00:1::1]/org/repo/commit/{sha}"),
        format!("https://100.64.0.1/org/repo/commit/{sha}"),
        format!("https://github.com:abc/gyldlab/keld/commit/{sha}"),
        format!("https://github.com:65536/gyldlab/keld/commit/{sha}"),
        format!("https://raw.githubusercontent.com./gyldlab/keld/main/{hexfile}"),
    ] {
        let mut record = parse_ok();
        record.evidence_uri = uri.clone();
        let err = score(&denom, &[record], AS_OF).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

#[test]
fn rejects_normalization_bypasses_at_parse_and_score() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let original = format!("https://github.com/gyldlab/keld/commit/{sha}");
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for uri in [
        format!("https://example.com/{sha}/../main"),
        format!("https://example.com/{sha}/%2e%2e/main"),
        format!("https://%31%32%37.0.0.1/commit/{sha}"),
        format!("https://%6cocalhost./commit/{sha}"),
        format!("https://github.com/org/repo/%62lob/main/{sha}"),
        format!("https://raw%2egithubusercontent.com/org/repo/main/{sha}"),
        format!("https://example.com/{sha}%2f../main"),
        format!("https://example.com/{sha}/%252e%252e/main"),
        format!("https://example.com/{sha}/..\\main"),
        format!("https://127.1/commit/{sha}"),
    ] {
        let json = valid_evidence_json().replace(&original, &uri.replace('\\', "\\\\"));
        let err = parse_evidence(json.as_bytes()).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        let mut record = parse_ok();
        record.evidence_uri = uri.clone();
        let err = score(&denom, &[record], AS_OF).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

#[test]
fn returns_normalized_public_pinned_url() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let original = format!("https://github.com/gyldlab/keld/commit/{sha}");
    let uri =
        format!("https://GITHUB.COM:443/unused/../gyldlab/keld/blob/{sha}/a%20b?download=1#L1");
    let json = valid_evidence_json().replace(&original, &uri);
    let record = parse_evidence(json.as_bytes()).expect("normalized public pin");
    let canonical = format!("https://github.com/gyldlab/keld/blob/{sha}/a%20b?download=1#L1");
    assert_eq!(record.evidence_uri(), canonical);
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    score(&denom, &[record], AS_OF).expect("canonical HTTPS evidence scores");
    let json = valid_evidence_json().replace(&original, &canonical);
    let reparsed = parse_evidence(json.as_bytes()).expect("canonical evidence reparses");
    assert_eq!(reparsed.evidence_uri(), canonical);
    score(&denom, &[reparsed], AS_OF).expect("reparsed evidence scores");
}

#[test]
fn preserves_encoded_literals_in_pinned_public_paths() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for prefix in [
        format!("https://github.com/gyldlab/keld/blob/{sha}"),
        format!("https://github.com/gyldlab/keld/tree/{sha}"),
        format!("https://github.com/gyldlab/keld/raw/{sha}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/{sha}"),
        format!("https://gitlab.com/org/repo/-/blob/{sha}"),
        format!("https://bitbucket.org/org/repo/src/{sha}"),
        format!("https://example.com/evidence/{sha}"),
    ] {
        for path in [
            "docs/issue%23notes.md",
            "docs/question%3Fnotes.md",
            "docs/100%25ready.md",
            "issue%23notes/question%3Fnotes/100%25ready/README.md",
        ] {
            assert_canonical_pinned_uri(&denom, &format!("{prefix}/{path}"));
        }
    }
}

#[test]
fn preserves_encoded_filename_bytes_in_pinned_public_paths() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for prefix in [
        format!("https://github.com/gyldlab/keld/blob/{sha}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/{sha}"),
    ] {
        for path in ["docs/Caf%C3%A9.md", "docs/file%FF.md", "dir%FE/file%FF.md"] {
            assert_canonical_pinned_uri(&denom, &format!("{prefix}/{path}"));
        }
    }
}

#[test]
fn preserves_backslash_data_in_pinned_url_query_and_fragment() {
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    let pin = parse_ok();
    for suffix in [
        r"?label=chapter\section",
        r"#chapter\section?note",
        "?label=chapter%5Csection",
        "#chapter%5Csection",
    ] {
        assert_canonical_pinned_uri(&denom, &format!("{}{suffix}", pin.evidence_uri()));
    }
}

#[test]
fn preserves_literal_percent_hex_filenames_after_one_decode() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for prefix in [
        format!("https://github.com/gyldlab/keld/blob/{sha}"),
        format!("https://github.com/gyldlab/keld/tree/{sha}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/{sha}"),
        format!("https://gitlab.com/org/repo/-/blob/{sha}"),
        format!("https://example.com/evidence/{sha}"),
    ] {
        for file in [
            "docs/100%25a1.md",
            "docs/file%25AB.txt",
            "docs/report%25deadbeef.txt",
            "docs/100%25ready.md",
        ] {
            assert_canonical_pinned_uri(&denom, &format!("{prefix}/{file}"));
        }
    }
}

#[test]
fn preserves_generic_host_ref_named_directories_after_pin() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let sha64 = format!("{sha}0123456789abcdef01234567");
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");

    for pin in [sha, sha64.as_str()] {
        for path in [
            "docs/%62lob/main.md",
            "docs/%2562lob/main.md",
            "docs/%74ree/main.md",
            "docs/%2574ree/main.md",
            "docs/%72aw/main.md",
            "docs/%2572aw/main.md",
            "docs/blob/main.md",
            "docs/tree/main.md",
            "docs/raw/main.md",
            "docs/commits/main.md",
            "docs/%63ommits/main.md",
            "docs/%61pi/main.md",
        ] {
            assert_canonical_pinned_uri(
                &denom,
                &format!("https://example.com/evidence/{pin}/{path}"),
            );
        }
    }
}

#[test]
fn generic_host_mutable_ref_before_pin_stays_rejected() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for route in [
        "blob/main",
        "tree/main",
        "raw/main",
        "%62lob/main",
        "%2562lob/main",
        "%74ree/main",
        "%2572aw/main",
    ] {
        let uri = format!("https://example.com/evidence/{route}/{sha}");
        let mut envelope: serde_json::Value =
            serde_json::from_str(&valid_evidence_json()).expect("valid evidence envelope");
        envelope["evidence_uri"] = serde_json::Value::String(uri.clone());
        let input = serde_json::to_vec(&envelope).expect("serialize evidence input");
        let err = parse_evidence(&input).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");

        let mut constructed = parse_ok();
        constructed.evidence_uri = uri.clone();
        let err = score(&denom, &[constructed], AS_OF).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

#[test]
fn generic_host_sha_looking_project_names_do_not_pin_mutable_provider_routes() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let project = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for uri in [
        format!("https://gitlab.gnome.org/team/{project}/-/blob/main/README.md"),
        format!("https://gitlab.gnome.org/team/{project}/-/raw/release/{sha}"),
        format!("https://gitea.example.com/team/{project}/blob/main/README.md"),
        format!("https://gitea.example.com/team/{project}/%62lob/main/README.md"),
        format!("https://gitea.example.com/team/{project}/%2562lob/main/README.md"),
        format!("https://gitea.example.com/team/{project}/src/branch/main/README.md"),
        format!("https://gitea.example.com/team/{project}/%73rc/branch/main/README.md"),
        format!("https://gitea.example.com/team/{project}/%2573rc/%62ranch/main/README.md"),
        format!("https://gitea.example.com/team/{project}/src/tag/latest/README.md"),
        format!("https://gitea.example.com/team/{project}/src/commit/main/README.md"),
        format!("https://git.example.com/team/{project}/src/main/README.md"),
    ] {
        let mut envelope: serde_json::Value =
            serde_json::from_str(&valid_evidence_json()).expect("valid evidence envelope");
        envelope["evidence_uri"] = serde_json::Value::String(uri.clone());
        let input = serde_json::to_vec(&envelope).expect("serialize evidence input");
        let err = parse_evidence(&input).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");

        let mut constructed = parse_ok();
        constructed.evidence_uri = uri.clone();
        let err = score(&denom, &[constructed], AS_OF).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
    // A real immutable provider revision remains valid even if the
    // project slug happens to resemble a SHA.
    assert_canonical_pinned_uri(
        &denom,
        &format!("https://gitlab.gnome.org/team/{project}/-/blob/{sha}/docs/%62lob/main.md"),
    );
    assert_canonical_pinned_uri(
        &denom,
        &format!("https://gitea.example.com/team/{project}/blob/{sha}/docs/%62lob/main.md"),
    );
    assert_canonical_pinned_uri(
        &denom,
        &format!("https://gitea.example.com/team/{project}/src/commit/{sha}/README.md"),
    );
    assert_canonical_pinned_uri(
        &denom,
        &format!("https://git.example.com/team/{project}/src/{sha}/README.md"),
    );
}

#[test]
fn preserves_ref_named_directories_below_immutable_git_refs() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for prefix in [
        format!("https://github.com/gyldlab/keld/blob/{sha}"),
        format!("https://github.com/gyldlab/keld/tree/{sha}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/{sha}"),
        format!("https://gitlab.com/org/repo/-/blob/{sha}"),
        format!("https://bitbucket.org/org/repo/src/{sha}"),
    ] {
        for file in [
            "docs/%62lob/main.md",
            "docs/%74ree/main.md",
            "docs/%72aw/main.md",
            "docs/%61pi/main.md",
        ] {
            assert_canonical_pinned_uri(&denom, &format!("{prefix}/{file}"));
        }
    }
}

#[test]
fn rejects_multi_encoded_path_navigation_and_mutable_ref_routes() {
    let sha = "67f39cdc898254f1e0c9cd50800f242ae7a4c493";
    let denom = parse_denominator(valid_denominator_json().as_bytes()).expect("denominator");
    for uri in [
        format!("https://github.com/gyldlab/keld/blob/{sha}/docs/%252e%252e/file.md"),
        format!("https://github.com/gyldlab/keld/blob/{sha}/docs/%25252e%25252e/file.md"),
        format!("https://github.com/gyldlab/keld/blob/{sha}/docs/file%252fother.md"),
        format!("https://github.com/gyldlab/keld/blob/{sha}/docs/file%255cother.md"),
        format!("https://github.com/gyldlab/keld/blob/{sha}/docs/file%2500other.md"),
        format!("https://github.com/gyldlab/keld/%62lob/main/{sha}"),
        format!("https://github.com/gyldlab/keld/blob/main/{sha}"),
        format!("https://raw.githubusercontent.com/gyldlab/keld/main/{sha}"),
        format!("https://gitlab.com/org/repo/-/blob/main/docs/-/blob/{sha}/README.md"),
        format!("https://github.com/gyldlab/keld/blob/%2536{}", &sha[1..]),
        format!(
            "https://raw.githubusercontent.com/gyldlab/keld/%2536{}/file.md",
            &sha[1..]
        ),
        format!("https://example.com/evidence/%2536{}/file.md", &sha[1..]),
    ] {
        let mut envelope: serde_json::Value =
            serde_json::from_str(&valid_evidence_json()).expect("valid evidence envelope");
        envelope["evidence_uri"] = serde_json::Value::String(uri.clone());
        let input = serde_json::to_vec(&envelope).expect("serialize evidence input");
        let err = parse_evidence(&input).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
        let mut constructed = parse_ok();
        constructed.evidence_uri = uri.clone();
        let err = score(&denom, &[constructed], AS_OF).expect_err(&uri);
        assert_eq!(err.code(), "KELD-COMPAT-007", "{uri}");
    }
}

fn assert_canonical_pinned_uri(denom: &Denominator, uri: &str) {
    let mut envelope: serde_json::Value =
        serde_json::from_str(&valid_evidence_json()).expect("valid evidence envelope");
    envelope["evidence_uri"] = serde_json::Value::String(uri.to_owned());
    let input = serde_json::to_vec(&envelope).expect("serialize evidence input");
    let record = parse_evidence(&input).unwrap_or_else(|err| panic!("{uri}: {err}"));
    assert_eq!(record.evidence_uri(), uri);
    envelope["evidence_uri"] = serde_json::Value::String(record.evidence_uri().to_owned());
    let serialized = serde_json::to_vec(&envelope).expect("serialize canonical evidence");
    let roundtrip = parse_evidence(&serialized).expect("evidence URI round trip");
    assert_eq!(roundtrip.evidence_uri(), uri);

    score(denom, &[record], AS_OF).expect("parsed URI is eligible for scoring");
    let mut constructed = parse_ok();
    constructed.evidence_uri = uri.to_owned();
    score(denom, &[constructed], AS_OF).expect("constructed URI is eligible for scoring");
}
