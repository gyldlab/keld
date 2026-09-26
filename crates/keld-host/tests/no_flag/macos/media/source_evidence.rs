use std::process::Command;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_probe_sheet_count(
    report: &std::collections::BTreeMap<String, String>,
) -> usize {
    assert_eq!(
        report.get("probe_scope").map(String::as_str),
        Some("process-wide")
    );
    report
        .get("probe_sheet_count")
        .expect("signed-host sheet count")
        .parse()
        .expect("numeric signed-host sheet count")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_probe_tcc_authorized(
    report: &std::collections::BTreeMap<String, String>,
    kind: &str,
) -> bool {
    let key = match kind {
        "camera" => "probe_camera_tcc",
        "microphone" => "probe_microphone_tcc",
        _ => panic!("unknown media kind in TCC probe"),
    };
    report.get(key).map(String::as_str) == Some("3")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_label_from_hex(value: &str) -> String {
    let bytes = value.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "device label hex has an odd length");
    let decoded = bytes
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("ASCII device label hex");
            u8::from_str_radix(pair, 16).expect("valid device label hex")
        })
        .collect::<Vec<_>>();
    String::from_utf8(decoded).expect("UTF-8 device label")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn camo_extension_matches_reviewed_version() -> bool {
    let extension = "/Applications/Camo Studio.app/Contents/Library/SystemExtensions/com.reincubate.macos.cam.avextension.systemextension/Contents/Info.plist";
    let read_plist = |key| {
        Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}"), extension])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let extensions = Command::new("/usr/bin/systemextensionsctl")
        .arg("list")
        .output()
        .ok()
        .filter(|output| output.status.success());
    read_plist("CFBundleShortVersionString").as_deref() == Some("2.4.0")
        && read_plist("CFBundleVersion").as_deref() == Some("17515")
        && extensions.is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.trim_start().starts_with('*')
                    && line.contains("Q248YREB53")
                    && line.contains("com.reincubate.macos.cam.avextension (2.4.0/17515)")
                    && line.contains("[activated enabled]")
            })
        })
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn selected_media_source_class(kind: &str, label: &str) -> &'static str {
    let output = Command::new("/usr/sbin/system_profiler")
        .args(["SPCameraDataType", "SPAudioDataType", "-json"])
        .output()
        .expect("read independent OS media-device inventory");
    assert!(
        output.status.success(),
        "system_profiler media census failed"
    );
    let inventory: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse OS media-device inventory");
    match kind {
        "camera" => {
            let camera = inventory["SPCameraDataType"]
                .as_array()
                .and_then(|devices| {
                    devices
                        .iter()
                        .find(|device| device["_name"].as_str() == Some(label))
                })
                .expect("selected camera label appears in OS device inventory");
            if label == "Camo Camera"
                && camera["spcamera_unique-id"].as_str() == Some("Camo")
                && camo_extension_matches_reviewed_version()
            {
                "os-virtual-camo"
            } else {
                "camera-inventory-matched-unclassified"
            }
        }
        "microphone" => {
            let audio = inventory["SPAudioDataType"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|section| section["_items"].as_array().into_iter().flatten())
                .find(|device| {
                    device["_name"].as_str() == Some(label)
                        && device["coreaudio_device_input"].as_u64().unwrap_or(0) > 0
                })
                .expect("selected microphone label appears as an OS input device");
            match audio["coreaudio_device_transport"].as_str() {
                Some("coreaudio_device_type_builtin") => "physical-builtin",
                Some("coreaudio_device_type_virtual") => "os-virtual",
                _ => "audio-inventory-matched-unclassified",
            }
        }
        _ => panic!("unknown media source kind"),
    }
}
