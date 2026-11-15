//! Reject untrusted evidence locations and accept immutable, pinned evidence.

use super::{
    AS_OF, DIGEST_A, parse_denominator, parse_evidence, parse_ok, score, valid_denominator_json,
    valid_evidence_json,
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
