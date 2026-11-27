//! External public-contract checks of the existing closed macOS metadata codec.
//! App metadata syntax is not a publisher-identity or Strict-profile admission.

#![cfg(target_os = "macos")]

use keld_pack::{
    MACOS_APP_RELATIVE_PATH, MACOS_BUN_RELATIVE_PATH, MACOS_HOST_EXECUTABLE, MACOS_LOCAL_TARGET,
    MAX_MACOS_METADATA_BYTES, MacosBundleMetadata,
};

fn fixture() -> MacosBundleMetadata {
    MacosBundleMetadata {
        app_id: "org.example.local".to_owned(),
        name: "Draw & Test <Editor>".to_owned(),
        version: "1.2.3".to_owned(),
        bundle_version: "4".to_owned(),
        minimum_system_version: "13.0".to_owned(),
        icon: true,
        bun_sha256: [0xab; 32],
    }
}

#[test]
fn public_metadata_codec_preserves_the_fixed_bundle_shape() {
    assert_eq!(MACOS_LOCAL_TARGET, "macos-arm64");
    assert_eq!(MACOS_HOST_EXECUTABLE, "keld-host");
    assert_eq!(MACOS_BUN_RELATIVE_PATH, "Contents/Helpers/bun");
    assert_eq!(MACOS_APP_RELATIVE_PATH, "Contents/Resources/app");

    let wanted = fixture();
    let plist = wanted.to_plist().expect("qualified representation");
    assert!(plist.len() <= MAX_MACOS_METADATA_BYTES);
    let encoded = std::str::from_utf8(&plist).expect("UTF-8");
    assert!(encoded.contains("<key>CFBundleExecutable</key><string>keld-host</string>"));
    assert!(encoded.contains("<key>target</key><string>macos-arm64</string>"));
    assert!(encoded.contains("Draw &amp; Test &lt;Editor&gt;"));
    assert!(encoded.contains(&"ab".repeat(32)));
    assert_eq!(
        MacosBundleMetadata::from_plist(&plist),
        Ok(wanted),
        "public parser must round-trip the exact signed-resource representation"
    );
}

#[test]
fn public_metadata_decoder_refuses_wrong_targets_keys_and_trailing_bytes() {
    let good = String::from_utf8(fixture().to_plist().expect("fixture")).expect("UTF-8");
    let wrong_target = good.replacen(
        "<key>target</key><string>macos-arm64</string>",
        "<key>target</key><string>macos-x64</string>",
        1,
    );
    let wrong_host = good.replacen(
        "<key>CFBundleExecutable</key><string>keld-host</string>",
        "<key>CFBundleExecutable</key><string>foreign-host</string>",
        1,
    );
    let duplicate_schema = good.replacen(
        "<key>schema</key>",
        "<key>schema</key><integer>1</integer><key>schema</key>",
        1,
    );
    let unknown_field = good.replacen(
        "<key>schema</key>",
        "<key>unexpected</key><integer>1</integer><key>schema</key>",
        1,
    );
    let trailing = format!("{good}<unexpected/>");
    for changed in [
        wrong_target,
        wrong_host,
        duplicate_schema,
        unknown_field,
        trailing,
    ] {
        assert_ne!(changed, good, "negative control must change input");
        let error = MacosBundleMetadata::from_plist(changed.as_bytes())
            .expect_err("malformed signed metadata must refuse");
        assert_eq!(error.code(), "KELD-PACK-013");
        assert!(error.to_string().starts_with("KELD-PACK-013:"));
    }
    let oversize = vec![b'x'; MAX_MACOS_METADATA_BYTES + 1];
    assert_eq!(
        MacosBundleMetadata::from_plist(&oversize)
            .expect_err("oversized resource must refuse")
            .code(),
        "KELD-PACK-013"
    );
}

#[test]
fn public_metadata_authoring_rejects_unrepresentable_input() {
    let mut invalid_id = fixture();
    invalid_id.app_id = "org.example/escape".to_owned();
    assert_eq!(
        invalid_id
            .to_plist()
            .expect_err("unsupported bundle ID syntax")
            .code(),
        "KELD-PACK-013"
    );

    let mut invalid_release = fixture();
    invalid_release.version = "1.2.3-beta".to_owned();
    assert_eq!(
        invalid_release
            .to_plist()
            .expect_err("Apple short-version shape must refuse")
            .code(),
        "KELD-PACK-013"
    );

    let mut invalid_name = fixture();
    invalid_name.name = "control\ncharacter".to_owned();
    assert_eq!(
        invalid_name
            .to_plist()
            .expect_err("control bytes must refuse")
            .code(),
        "KELD-PACK-013"
    );
}
