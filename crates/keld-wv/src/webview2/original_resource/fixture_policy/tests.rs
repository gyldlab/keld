#![allow(clippy::expect_used)] // Exact fixed inputs and pure contract assertions.
use super::{
    BASE_POLICY, INLINE_DOCUMENT_BYTES, INLINE_DOCUMENT_SHA256, INLINE_POLICY,
    INLINE_SCRIPT_SHA256, InlineFixture, SCRIPT_SOURCE_TOKEN,
};
use crate::preload_image::{AppPreloadBudget, AppPreloadImage};
use crate::resource_intake::{
    MAX_PENDING_METADATA_BYTES, ORIGINAL_WEBAPP_LIMITS, pending_metadata_charge,
};
use crate::resource_source::{SelectedRead, VerifiedBytes};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

const PRELOAD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../keld-host/tests/no_flag/windows/fixtures/original-bridge-preload.fixture"
));
const HTML: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../keld-host/tests/no_flag/windows/fixtures/original-inline-editor.html"
));
const SCRIPT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../keld-host/tests/no_flag/windows/fixtures/original-bridge-probe.js"
));

fn image(bytes: Vec<u8>) -> AppPreloadImage {
    let digest = Sha256::digest(&bytes).into();
    AppPreloadBudget::default()
        .reserve_window()
        .expect("actual image account")
        .from_qualified_graph(
            bytes,
            digest,
            vec![String::from("--initial-adaptive-colors=auto")],
            BTreeMap::from([
                (String::from("bun"), String::from("fixture-bun")),
                (String::from("keld"), String::from("fixture-host")),
            ]),
        )
        .expect("actual checked immutable image")
}

fn selected() -> SelectedRead {
    SelectedRead {
        path: String::from("client/assets/editor.html"),
        size: HTML.len() as u64,
        sha256: Sha256::digest(HTML).into(),
        mime: "text/html; charset=utf-8",
    }
}

fn body() -> VerifiedBytes {
    VerifiedBytes {
        bytes: HTML.to_vec(),
        mime: "text/html; charset=utf-8",
    }
}

#[test]
fn inline_fixture_is_one_literal_copy_of_the_existing_first_author_probe() {
    let html = std::str::from_utf8(HTML).expect("fixed UTF8 HTML");
    assert_eq!(html.matches("<script>").count(), 1);
    assert_eq!(html.matches("</script>").count(), 1);
    assert!(!html.contains("<script src="));
    let literal = html
        .split_once("<script>")
        .expect("one first literal script")
        .1
        .split_once("</script>")
        .expect("one script ending")
        .0;
    assert_eq!(literal.as_bytes(), SCRIPT, "probe body is unchanged");
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(SCRIPT)),
        INLINE_SCRIPT_SHA256
    );
    assert_eq!(HTML.len(), INLINE_DOCUMENT_BYTES);
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(HTML)),
        INLINE_DOCUMENT_SHA256
    );
    assert_eq!(INLINE_POLICY.matches(SCRIPT_SOURCE_TOKEN).count(), 1);
    assert_eq!(
        INLINE_POLICY.replacen(SCRIPT_SOURCE_TOKEN, "", 1),
        BASE_POLICY
    );
    assert!(INLINE_POLICY.contains("frame-src 'none'"));
    assert!(!INLINE_POLICY.contains("'unsafe-inline'"));
}

#[test]
fn fixed_inline_policy_requires_the_actual_original_image_and_verified_body() {
    let admitted = image(PRELOAD.to_vec());
    let policy = InlineFixture::capture(&admitted);
    assert_eq!(
        policy.select(BASE_POLICY, &selected(), &body()),
        INLINE_POLICY
    );
    let mut changed_image = PRELOAD.to_vec();
    changed_image.extend_from_slice(b"\n");
    let foreign = image(changed_image);
    assert_eq!(
        InlineFixture::capture(&foreign).select(BASE_POLICY, &selected(), &body()),
        BASE_POLICY,
        "a different admitted image cannot select the fixture policy"
    );
    let mut changed_body = body();
    changed_body.bytes[0] ^= 1;
    assert_eq!(
        policy.select(BASE_POLICY, &selected(), &changed_body),
        BASE_POLICY,
        "same-length changed bytes cannot select the fixture policy"
    );
}

#[test]
fn fixed_inline_policy_refuses_other_catalog_read_size_digest_and_mime() {
    let admitted = image(PRELOAD.to_vec());
    let policy = InlineFixture::capture(&admitted);
    let other = "default-src 'none'";
    assert_eq!(policy.select(other, &selected(), &body()), other);
    let mut read = selected();
    read.sha256[0] ^= 1;
    assert_eq!(policy.select(BASE_POLICY, &read, &body()), BASE_POLICY);
    let mut read = selected();
    read.size += 1;
    assert_eq!(policy.select(BASE_POLICY, &read, &body()), BASE_POLICY);
    let mut read = selected();
    read.mime = "text/plain; charset=utf-8";
    assert_eq!(policy.select(BASE_POLICY, &read, &body()), BASE_POLICY);
    let mut wrong = body();
    wrong.mime = "text/plain; charset=utf-8";
    assert_eq!(policy.select(BASE_POLICY, &selected(), &wrong), BASE_POLICY);
}

#[test]
fn fixed_inline_header_fits_the_existing_pending_and_view_metadata_charges() {
    let admitted = image(PRELOAD.to_vec());
    let policy = InlineFixture::capture(&admitted);
    let maximum = policy.maximum_header_len(BASE_POLICY.len());
    assert_eq!(maximum, INLINE_POLICY.len());
    let charge = pending_metadata_charge(
        ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units + ORIGINAL_WEBAPP_LIMITS.max_header_utf16_units,
        4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        1024,
        4 * maximum,
        ORIGINAL_WEBAPP_LIMITS,
    )
    .expect("fixed header fits the same pending ceiling before capture");
    assert!(charge <= MAX_PENDING_METADATA_BYTES);
    assert!(
        std::mem::size_of::<super::super::ResourceView>() + 2 * std::mem::size_of::<usize>()
            <= 2048
    );
}
